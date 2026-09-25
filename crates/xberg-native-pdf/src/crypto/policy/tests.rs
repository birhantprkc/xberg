use super::*;

/// Compat allows every (alg, use_) — behaviour-preserving default.
#[test]
fn compat_allows_everything() {
    let p = SecurityPolicy::compat();
    for &a in &AlgorithmId::ALL {
        for u in [AlgorithmUse::Read, AlgorithmUse::Write] {
            assert_eq!(p.evaluate(a, u), Decision::Allow, "compat {a:?} {u:?}");
        }
    }
}

/// Strict: every algorithm is readable; only FIPS-approved
/// primitives are writable.
#[test]
fn strict_read_all_write_fips_only() {
    let p = SecurityPolicy::strict();
    for &a in &AlgorithmId::ALL {
        assert_eq!(p.evaluate(a, AlgorithmUse::Read), Decision::Allow, "strict R {a:?}");
        let want = if a.is_fips_approved() {
            Decision::Allow
        } else {
            Decision::Deny
        };
        assert_eq!(p.evaluate(a, AlgorithmUse::Write), want, "strict W {a:?}");
    }
    assert_eq!(p.evaluate(AlgorithmId::HashMd5, AlgorithmUse::Write), Decision::Deny);
    assert_eq!(p.evaluate(AlgorithmId::CipherRc4, AlgorithmUse::Write), Decision::Deny);
    assert_eq!(
        p.evaluate(AlgorithmId::SigRsaPkcs1v15Sha1, AlgorithmUse::Write),
        Decision::Deny
    );
    assert_eq!(
        p.evaluate(AlgorithmId::CipherAes256Cbc, AlgorithmUse::Write),
        Decision::Allow
    );
    assert_eq!(
        p.evaluate(AlgorithmId::SigEcdsaP256Sha256, AlgorithmUse::Write),
        Decision::Allow
    );
}

/// FipsStrict: non-approved denied both directions, except SHA-1
/// *hash* read (historical-signature verification, SP 800-131A).
#[test]
fn fips_strict_matrix() {
    let p = SecurityPolicy::fips_strict();
    for &a in &AlgorithmId::ALL {
        if a.is_fips_approved() {
            assert_eq!(p.evaluate(a, AlgorithmUse::Read), Decision::Allow, "fips R {a:?}");
            assert_eq!(p.evaluate(a, AlgorithmUse::Write), Decision::Allow, "fips W {a:?}");
        } else {
            assert_eq!(p.evaluate(a, AlgorithmUse::Write), Decision::Deny, "fips W {a:?}");
            let want_read = if a == AlgorithmId::HashSha1 {
                Decision::Allow
            } else {
                Decision::Deny
            };
            assert_eq!(p.evaluate(a, AlgorithmUse::Read), want_read, "fips R {a:?}");
        }
    }
}

#[test]
fn deny_override_is_terminal_even_over_compat_allow() {
    let p = SecurityPolicy::builder(PolicyMode::Compat)
        .deny(AlgorithmId::CipherRc4, AlgorithmUse::Write)
        .build();
    assert_eq!(p.evaluate(AlgorithmId::CipherRc4, AlgorithmUse::Write), Decision::Deny);
    assert_eq!(p.evaluate(AlgorithmId::CipherRc4, AlgorithmUse::Read), Decision::Allow);
}

#[test]
fn allow_override_beats_strict_write_deny() {
    let p = SecurityPolicy::builder(PolicyMode::Strict)
        .allow(AlgorithmId::CipherRc4, AlgorithmUse::Write)
        .build();
    assert_eq!(p.evaluate(AlgorithmId::CipherRc4, AlgorithmUse::Write), Decision::Allow);
}

#[test]
fn fips_strict_ignores_allow_override_for_non_approved() {
    let p = SecurityPolicy::builder(PolicyMode::FipsStrict)
        .allow(AlgorithmId::CipherRc4, AlgorithmUse::Read)
        .allow(AlgorithmId::CipherRc4, AlgorithmUse::Write)
        .build();
    assert_eq!(p.evaluate(AlgorithmId::CipherRc4, AlgorithmUse::Read), Decision::Deny);
    assert_eq!(p.evaluate(AlgorithmId::CipherRc4, AlgorithmUse::Write), Decision::Deny);
    let p2 = SecurityPolicy::builder(PolicyMode::FipsStrict)
        .allow(AlgorithmId::CipherAes256Cbc, AlgorithmUse::Write)
        .build();
    assert_eq!(
        p2.evaluate(AlgorithmId::CipherAes256Cbc, AlgorithmUse::Write),
        Decision::Allow
    );
}

#[test]
fn strict_min_bits_256_forbids_aes128_write_allows_read() {
    let p = SecurityPolicy::builder(PolicyMode::Strict)
        .min_security_bits(256)
        .build();
    assert_eq!(
        p.evaluate(AlgorithmId::CipherAes128Cbc, AlgorithmUse::Write),
        Decision::Deny
    );
    assert_eq!(
        p.evaluate(AlgorithmId::CipherAes128Cbc, AlgorithmUse::Read),
        Decision::Allow
    );
    assert_eq!(
        p.evaluate(AlgorithmId::CipherAes256Cbc, AlgorithmUse::Write),
        Decision::Allow
    );
    let d = SecurityPolicy::strict();
    assert_eq!(
        d.evaluate(AlgorithmId::CipherAes128Cbc, AlgorithmUse::Write),
        Decision::Allow
    );
}

#[test]
fn parse_modes() {
    assert_eq!("compat".parse::<SecurityPolicy>().unwrap().mode(), PolicyMode::Compat);
    assert_eq!("strict".parse::<SecurityPolicy>().unwrap().mode(), PolicyMode::Strict);
    assert_eq!(
        "fips-strict".parse::<SecurityPolicy>().unwrap().mode(),
        PolicyMode::FipsStrict
    );
}

#[test]
fn parse_with_overrides_and_roundtrip() {
    let spec = "compat;deny:rc4@write;deny:md5@write";
    let p: SecurityPolicy = spec.parse().unwrap();
    assert_eq!(p.evaluate(AlgorithmId::CipherRc4, AlgorithmUse::Write), Decision::Deny);
    assert_eq!(p.evaluate(AlgorithmId::HashMd5, AlgorithmUse::Write), Decision::Deny);
    assert_eq!(p.evaluate(AlgorithmId::CipherRc4, AlgorithmUse::Read), Decision::Allow);
    let rendered = p.to_string();
    let reparsed: SecurityPolicy = rendered.parse().unwrap();
    assert_eq!(reparsed.to_string(), rendered);
}

#[test]
fn parse_tolerates_whitespace() {
    let p: SecurityPolicy = " strict ; deny: sha1 @ write ".parse().unwrap();
    assert_eq!(p.mode(), PolicyMode::Strict);
    assert_eq!(p.evaluate(AlgorithmId::HashSha1, AlgorithmUse::Write), Decision::Deny);
}

#[test]
fn parse_fails_closed_on_garbage() {
    for bad in [
        "",
        "garbage-mode",
        "compat;rc4@write",         // missing verb ~keep
        "compat;deny:rc4",          // missing @use ~keep
        "compat;deny:nope@write",   // unknown alg ~keep
        "compat;deny:rc4@sideways", // unknown use ~keep
        "compat;maybe:rc4@write",   // unknown verb ~keep
    ] {
        assert!(
            bad.parse::<SecurityPolicy>().is_err(),
            "spec {bad:?} must be a fail-closed parse error"
        );
    }
}

#[test]
fn log_audit_sink_does_not_panic() {
    let s = LogAuditSink;
    s.record(&AuditEvent {
        algorithm: AlgorithmId::CipherRc4,
        use_: AlgorithmUse::Write,
        decision: Decision::Deny,
        mode: PolicyMode::Strict,
    });
    NoopAuditSink.record(&AuditEvent {
        algorithm: AlgorithmId::HashSha256,
        use_: AlgorithmUse::Read,
        decision: Decision::Allow,
        mode: PolicyMode::Compat,
    });
}

#[test]
fn token_roundtrips_for_every_algorithm() {
    for &a in &AlgorithmId::ALL {
        assert_eq!(AlgorithmId::from_token(a.token()), Some(a), "token roundtrip {a:?}");
    }
    assert_eq!(AlgorithmId::from_token("does-not-exist"), None);
}

#[test]
fn kind_and_fips_classification_are_consistent() {
    assert_eq!(AlgorithmId::HashMd5.kind(), AlgorithmKind::Hash);
    assert_eq!(AlgorithmId::CipherRc4.kind(), AlgorithmKind::SymmetricCipher);
    assert_eq!(AlgorithmId::SigRsaPssSha256.kind(), AlgorithmKind::SignatureSign);
    assert!(!AlgorithmId::HashMd5.is_fips_approved());
    assert!(!AlgorithmId::HashSha1.is_fips_approved());
    assert!(!AlgorithmId::CipherRc4.is_fips_approved());
    assert!(!AlgorithmId::SigRsaPkcs1v15Sha1.is_fips_approved());
    assert!(AlgorithmId::HashSha256.is_fips_approved());
    assert!(AlgorithmId::CipherAes256Cbc.is_fips_approved());
    assert!(AlgorithmId::SigEcdsaP384Sha384.is_fips_approved());
}

#[test]
fn default_policy_is_compat() {
    assert_eq!(SecurityPolicy::default().mode(), PolicyMode::Compat);
}

#[test]
fn evaluate_token_fails_closed_on_unknown() {
    let p = SecurityPolicy::compat();
    assert_eq!(p.evaluate_token("aes256", AlgorithmUse::Write), Decision::Allow);
    assert_eq!(p.evaluate_token("kyber768", AlgorithmUse::Read), Decision::Deny);
    assert_eq!(p.unknown_algorithm_decision(), Decision::Deny);
}

#[test]
fn index_is_stable_and_unique() {
    for (i, &a) in AlgorithmId::ALL.iter().enumerate() {
        assert_eq!(a.index(), i, "index must match position in ALL for {a:?}");
    }
    let mut seen = [false; 64];
    for &a in &AlgorithmId::ALL {
        assert!(a.index() < 64);
        assert!(!seen[a.index()], "duplicate index for {a:?}");
        seen[a.index()] = true;
    }
}

#[test]
fn unknown_algorithm_decision_is_configurable() {
    let p = SecurityPolicy::builder(PolicyMode::Compat)
        .unknown_algorithm(Decision::Allow)
        .build();
    assert_eq!(p.evaluate_token("future-pqc", AlgorithmUse::Read), Decision::Allow);
}

#[test]
fn pqc_algorithm_ids_round_trip_and_are_fips_classified() {
    for (tok, sec) in [
        ("ml-dsa-44", 128u16),
        ("ml-dsa-65", 192),
        ("ml-dsa-87", 256),
        ("ml-kem-512", 128),
        ("ml-kem-768", 192),
        ("ml-kem-1024", 256),
    ] {
        let a = AlgorithmId::from_token(tok).unwrap_or_else(|| panic!("{tok} known"));
        assert_eq!(a.token(), tok, "token round-trips");
        assert!(a.is_fips_approved(), "{tok} is FIPS 203/204 approved");
        assert_eq!(a.min_security_bits(), sec, "{tok} NIST-level strength");
    }
    assert_eq!(
        AlgorithmId::from_token("ml-dsa-65").unwrap().kind(),
        AlgorithmKind::SignatureSign
    );
    assert_eq!(
        AlgorithmId::from_token("ml-kem-768").unwrap().kind(),
        AlgorithmKind::KeyDerivation
    );
    // Frozen indices preserved: the original 17 ids keep their
    // positions; PQC ids appended at 17..23. ~keep
    assert_eq!(AlgorithmId::HashMd5.index(), 0);
    assert_eq!(AlgorithmId::SigEcdsaP384Sha384.index(), 16);
    assert_eq!(AlgorithmId::SigMlDsa44.index(), 17);
    assert_eq!(AlgorithmId::KemMlKem1024.index(), 22);
}

#[test]
fn cnsa2_and_pqc_ready_modes_parse_and_govern() {
    for tok in ["cnsa2", "pqc-ready"] {
        let p: SecurityPolicy = tok.parse().expect("mode parses");
        assert_eq!(p.mode().token(), tok);
    }
    let cnsa2: SecurityPolicy = "cnsa2".parse().unwrap();
    let pqc: SecurityPolicy = "pqc-ready".parse().unwrap();

    assert_eq!(
        cnsa2.evaluate_token("rsa-pkcs1-sha256", AlgorithmUse::Read),
        Decision::Allow
    );

    assert_eq!(cnsa2.evaluate_token("ml-dsa-65", AlgorithmUse::Write), Decision::Allow);
    assert_eq!(cnsa2.evaluate_token("ml-dsa-87", AlgorithmUse::Write), Decision::Allow);
    assert_eq!(
        cnsa2.evaluate_token("rsa-pss-sha256", AlgorithmUse::Write),
        Decision::Deny
    );
    assert_eq!(cnsa2.evaluate_token("ml-dsa-44", AlgorithmUse::Write), Decision::Deny);
    assert_eq!(cnsa2.evaluate_token("md5", AlgorithmUse::Write), Decision::Deny);

    assert_eq!(pqc.evaluate_token("ml-dsa-44", AlgorithmUse::Write), Decision::Allow);
    assert_eq!(
        pqc.evaluate_token("rsa-pss-sha256", AlgorithmUse::Write),
        Decision::Allow
    );
    assert_eq!(pqc.evaluate_token("rc4", AlgorithmUse::Write), Decision::Deny);
}

#[test]
fn rsa_modulus_floor_defaults_and_enforcement() {
    // Per-mode defaults: Compat none; Strict/PqcReady 2048;
    // FipsStrict/Cnsa2 3072 (NIST SP 800-131A / CNSA 2.0). ~keep
    assert_eq!(SecurityPolicy::compat().min_rsa_modulus_bits(), 0);
    assert_eq!(SecurityPolicy::strict().min_rsa_modulus_bits(), 2048);
    assert_eq!(SecurityPolicy::fips_strict().min_rsa_modulus_bits(), 3072);
    assert_eq!(
        "pqc-ready".parse::<SecurityPolicy>().unwrap().min_rsa_modulus_bits(),
        2048
    );
    assert_eq!("cnsa2".parse::<SecurityPolicy>().unwrap().min_rsa_modulus_bits(), 3072);

    assert_eq!(SecurityPolicy::compat().rsa_modulus_allowed(1024), Decision::Allow);

    let strict = SecurityPolicy::strict();
    assert_eq!(strict.rsa_modulus_allowed(1024), Decision::Deny);
    assert_eq!(strict.rsa_modulus_allowed(2048), Decision::Allow);
    assert_eq!(strict.rsa_modulus_allowed(4096), Decision::Allow);

    let cnsa2: SecurityPolicy = "cnsa2".parse().unwrap();
    assert_eq!(cnsa2.rsa_modulus_allowed(2048), Decision::Deny);
    assert_eq!(cnsa2.rsa_modulus_allowed(3072), Decision::Allow);

    let p = SecurityPolicy::builder(PolicyMode::Strict)
        .min_rsa_modulus_bits(4096)
        .build();
    assert_eq!(p.rsa_modulus_allowed(3072), Decision::Deny);
    assert_eq!(p.rsa_modulus_allowed(4096), Decision::Allow);
    let none = SecurityPolicy::builder(PolicyMode::FipsStrict)
        .min_rsa_modulus_bits(0)
        .build();
    assert_eq!(none.rsa_modulus_allowed(1024), Decision::Allow);
}
