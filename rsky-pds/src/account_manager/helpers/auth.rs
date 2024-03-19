use crate::auth_verifier::AuthScope;
use crate::common::get_random_str;
use anyhow::Result;
use jwt_simple::prelude::*;
use secp256k1::Keypair;

pub struct CreateTokensOpts {
    pub did: String,
    pub jwt_key: Keypair,
    pub service_did: String,
    pub scope: Option<AuthScope>,
    pub jti: Option<String>,
    pub expires_in: Option<Duration>,
}

pub struct AuthToken {
    pub scope: AuthScope,
    pub sub: String,
    pub exp: Duration,
}

pub struct RefreshToken {
    pub scope: AuthScope, // AuthScope::Refresh
    pub sub: String,
    pub exp: Duration,
    pub jti: String,
}

#[derive(Serialize, Deserialize)]
pub struct CustomClaimObj {
    pub scope: String,
}

pub fn create_tokens(opts: CreateTokensOpts) -> Result<(String, String)> {
    let CreateTokensOpts {
        did,
        jwt_key,
        service_did,
        scope,
        jti,
        expires_in,
    } = opts;
    let access_jwt = create_access_token(CreateTokensOpts {
        did: did.clone(),
        jwt_key,
        service_did: service_did.clone(),
        scope,
        expires_in,
        jti: None,
    })?;
    let refresh_jwt = create_refresh_token(CreateTokensOpts {
        did,
        jwt_key,
        service_did,
        jti,
        expires_in,
        scope: None,
    })?;
    Ok((access_jwt, refresh_jwt))
}

pub fn create_access_token(opts: CreateTokensOpts) -> Result<String> {
    let CreateTokensOpts {
        did,
        jwt_key,
        service_did,
        scope,
        expires_in,
        ..
    } = opts;
    let scope = scope.unwrap_or_else(|| AuthScope::Access);
    let expires_in = expires_in.unwrap_or_else(|| Duration::from_hours(2));
    let claims = Claims::with_custom_claims(
        CustomClaimObj {
            scope: scope.as_str().to_owned(),
        },
        expires_in,
    )
    .with_audience(service_did)
    .with_subject(did);
    // alg ES256K
    let key = ES256kKeyPair::from_bytes(&*jwt_key.secret_bytes())?;
    let token = key.sign(claims)?;
    Ok(token)
}

pub fn create_refresh_token(opts: CreateTokensOpts) -> Result<String> {
    let CreateTokensOpts {
        did,
        jwt_key,
        service_did,
        jti,
        expires_in,
        ..
    } = opts;
    let jti = jti.unwrap_or_else(|| get_random_str());
    let expires_in = expires_in.unwrap_or_else(|| Duration::from_days(90));
    let claims = Claims::with_custom_claims(
        CustomClaimObj {
            scope: AuthScope::Refresh.as_str().to_owned(),
        },
        expires_in,
    )
    .with_audience(service_did)
    .with_subject(did)
    .with_jwt_id(jti);
    // alg ES256K
    let key = ES256kKeyPair::from_bytes(&*jwt_key.secret_bytes())?;
    let token = key.sign(claims)?;
    Ok(token)
}

// @NOTE unsafe for verification, should only be used w/ direct output from createRefreshToken() or createTokens()
pub fn decode_refresh_token(jwt: String, jwt_key: Keypair) -> Result<RefreshToken> {
    let key = ES256kKeyPair::from_bytes(&*jwt_key.secret_bytes())?;
    let public_key = key.public_key();
    let claims = public_key.verify_token::<CustomClaimObj>(&jwt, None)?;
    assert_eq!(claims.custom.scope, AuthScope::Refresh.as_str().to_owned(), "not a refresh token");
    Ok(RefreshToken{
        scope: AuthScope::from_str(&claims.custom.scope)?,
        sub: claims.subject.unwrap(),
        exp: claims.expires_at.unwrap(),
        jti: claims.jwt_id.unwrap()
    })
}
