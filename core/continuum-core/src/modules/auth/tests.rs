#![cfg(test)]
    use super::*;

    #[test]
    fn test_code_challenge_known_vector() {
        // RFC 7636 §Appendix B — verifier "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        // → challenge "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = ExternalWebviewAuthService::code_challenge(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_code_verifier_length() {
        let v = ExternalWebviewAuthService::generate_code_verifier();
        // base64url of 32 bytes = 43 chars (URL_SAFE_NO_PAD)
        assert_eq!(v.len(), 43);
    }

    #[test]
    fn test_token_expiry() {
        let now = crate::time::EpochMs::now().secs();

        let expired = TokenSet {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: now - 120, // 2 minutes ago
            token_type: "bearer".into(),
            scope: None,
        };
        assert!(expired.is_expired());

        let valid = TokenSet {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: now + 3600, // 1 hour from now
            token_type: "bearer".into(),
            scope: None,
        };
        assert!(!valid.is_expired());

        let unknown_expiry = TokenSet {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: 0,
            token_type: "bearer".into(),
            scope: None,
        };
        assert!(!unknown_expiry.is_expired());
    }

    #[tokio::test]
    async fn test_register_and_list_providers() {
        let svc = ExternalWebviewAuthService::new();
        svc.register_provider(OAuthClientConfig {
            provider_id: "test_provider".into(),
            client_id: "test_client_id".into(),
            client_secret: None,
            auth_url: "https://example.com/auth".into(),
            token_url: "https://example.com/token".into(),
            scopes: "read write".into(),
            redirect_port: 49999,
            revoke_url: None,
        })
        .await;

        let providers = svc.providers.read().await;
        assert!(providers.contains_key("test_provider"));
        assert_eq!(providers["test_provider"].client_id, "test_client_id");
    }

    #[tokio::test]
    async fn test_status_unauthenticated() {
        let svc = ExternalWebviewAuthService::new();
        let status = svc.token_status("github");
        assert!(!status.authenticated);
    }

    #[test]
    fn test_build_auth_url_contains_pkce_params() {
        let config = OAuthClientConfig {
            provider_id: "test".into(),
            client_id: "my_client".into(),
            client_secret: None,
            auth_url: "https://example.com/auth".into(),
            token_url: "https://example.com/token".into(),
            scopes: "read".into(),
            redirect_port: 47200,
            revoke_url: None,
        };
        let url = build_auth_url(&config, "test_challenge", "test_state");
        assert!(url.contains("code_challenge=test_challenge"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=test_state"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=my_client"));
    }
