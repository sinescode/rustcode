//! Property-based tests for core BlazeCode logic.
//!
//! Uses proptest to verify invariants across random inputs.

use blazecode_core::permission::wildcard_match;

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // ── Wildcard Matching Properties ─────────────────────────────

        #[test]
        fn wildcard_always_matches_star(input: String) {
            prop_assert!(wildcard_match(&input, "*"));
        }

        #[test]
        fn wildcard_exact_match(input: String) {
            prop_assume!(!input.contains('\\'));
            prop_assume!(!input.is_empty());
            prop_assert!(wildcard_match(&input, &input));
        }

        #[test]
        fn wildcard_prefix_match(mut input: String, suffix: String) {
            prop_assume!(!input.contains('\\'));
            if !input.is_empty() {
                let prefix = input.clone();
                input.push_str(&suffix);
                let pattern = format!("{}*", prefix);
                prop_assert!(wildcard_match(&input, &pattern));
            }
        }

        #[test]
        fn wildcard_suffix_match(prefix: String, mut input: String) {
            prop_assume!(!input.contains('\\'));
            if !input.is_empty() {
                let suffix = input.clone();
                input = format!("{}{}", prefix, input);
                let pattern = format!("*{}", suffix);
                prop_assert!(wildcard_match(&input, &pattern));
            }
        }

        #[test]
        fn wildcard_contains_match(prefix: String, mid: String, suffix: String) {
            prop_assume!(!mid.contains('\\'));
            prop_assume!(!mid.is_empty());
            let input = format!("{}{}{}", prefix, mid, suffix);
            let pattern = format!("*{}*", mid);
            prop_assert!(wildcard_match(&input, &pattern));
        }

        #[test]
        fn wildcard_question_matches_single_char(mut input: String, ch: char) {
            prop_assume!(!input.contains('\\'));
            let char_count = input.chars().count();
            if char_count >= 2 {
                // Find a char boundary near the middle of the string
                let char_pos = char_count / 2;
                let byte_pos = input.char_indices().nth(char_pos).map(|(i, _)| i).unwrap_or(0);
                let next_byte = input[byte_pos..].char_indices().nth(1).map(|(i, _)| byte_pos + i).unwrap_or(input.len());
                let expected = input.clone();
                input.replace_range(byte_pos..next_byte, &ch.to_string());
                let pattern = format!("{}?{}", &expected[..byte_pos], &expected[next_byte..]);
                prop_assert!(wildcard_match(&input, &pattern));
            }
        }

        #[test]
        fn wildcard_backslash_normalization(input: String) {
            let with_backslash = input.replace('/', "\\");
            let with_forward = input.replace('\\', "/");
            prop_assume!(with_backslash != with_forward);
            // Both should match the same patterns
            let pattern = with_forward.replace('\\', "/");
            prop_assert_eq!(
                wildcard_match(&with_backslash, &pattern),
                wildcard_match(&with_forward, &pattern)
            );
        }

        // ── ID Properties ────────────────────────────────────────────

        #[test]
        fn ids_are_unique(count: u32) {
            let count = count % 100 + 1;
            let mut ids = Vec::new();
            for _ in 0..count {
                let id = blazecode_core::id::ascending(
                    blazecode_core::id::IdPrefix::Message,
                    None,
                ).unwrap();
                ids.push(id);
            }
            ids.dedup();
            prop_assert_eq!(ids.len(), count as usize);
        }

        #[test]
        fn ids_are_ascending(count: u32) {
            let count = count % 100 + 2;
            let mut ids = Vec::new();
            for _ in 0..count {
                let id = blazecode_core::id::ascending(
                    blazecode_core::id::IdPrefix::Part,
                    None,
                ).unwrap();
                ids.push(id);
            }
            for i in 1..ids.len() {
                prop_assert!(ids[i] > ids[i-1], "IDs must be ascending: {} <= {}", ids[i], ids[i-1]);
            }
        }

        #[test]
        fn ids_have_correct_prefix(tag: String) {
            let prefix = match tag.to_lowercase().as_str() {
                "session" => blazecode_core::id::IdPrefix::Session,
                "message" => blazecode_core::id::IdPrefix::Message,
                "part" => blazecode_core::id::IdPrefix::Part,
                _ => blazecode_core::id::IdPrefix::Event,
            };
            let id = blazecode_core::id::ascending(prefix, None).unwrap();
            let expected_prefix = match prefix {
                blazecode_core::id::IdPrefix::Session => "ses_",
                blazecode_core::id::IdPrefix::Message => "msg_",
                blazecode_core::id::IdPrefix::Part => "part_",
                blazecode_core::id::IdPrefix::Event => "evt_",
                _ => "evt_",
            };
            prop_assert!(id.starts_with(expected_prefix), "ID {} should start with {}", id, expected_prefix);
        }

        // ── Serde Roundtrip Properties ───────────────────────────────

        #[test]
        fn session_info_roundtrip_preserves_data(
            id: String,
            slug: String,
            directory: String,
        ) {
            let info = blazecode_core::session::SessionInfo {
                id: id.clone(),
                slug,
                directory,
                ..Default::default()
            };
            let json = serde_json::to_string(&info).unwrap();
            let restored: blazecode_core::session::SessionInfo =
                serde_json::from_str(&json).unwrap();
            prop_assert_eq!(restored.id, id);
        }
    }

    // ── Encryption Roundtrip ──────────────────────────────────────────
    #[test]
    fn encryption_roundtrip_arbitrary_bytes() {
        use blazecode_core::encryption::hmac::{EncryptionService, KEY_LENGTH};
        let key = [0xABu8; KEY_LENGTH];
        let svc = EncryptionService::new(key);

        // Test with empty string
        let encrypted = svc.encrypt("").unwrap();
        let decrypted = svc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "");

        // Test with unicode
        let encrypted = svc.encrypt("Hello 世界! 🎉").unwrap();
        let decrypted = svc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, "Hello 世界! 🎉");

        // Test with very long string
        let long: String = (0..10000).map(|i| (b'a' + (i % 26) as u8) as char).collect();
        let encrypted = svc.encrypt(&long).unwrap();
        let decrypted = svc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, long);

        // Test tamper detection
        let original = svc.encrypt("secret data").unwrap();
        let tampered = format!("x{}", &original[1..]);
        assert!(svc.decrypt(&tampered).is_err());
    }
}
