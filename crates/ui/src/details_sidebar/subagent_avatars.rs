const AVATAR_MODULUS: u64 = 2_147_483_647;
const BLOBATAR_AVATARS: [&str; 28] = [
    "icons/subagents/blobatar/00.svg",
    "icons/subagents/blobatar/01.svg",
    "icons/subagents/blobatar/02.svg",
    "icons/subagents/blobatar/03.svg",
    "icons/subagents/blobatar/04.svg",
    "icons/subagents/blobatar/05.svg",
    "icons/subagents/blobatar/06.svg",
    "icons/subagents/blobatar/07.svg",
    "icons/subagents/blobatar/08.svg",
    "icons/subagents/blobatar/09.svg",
    "icons/subagents/blobatar/10.svg",
    "icons/subagents/blobatar/11.svg",
    "icons/subagents/blobatar/12.svg",
    "icons/subagents/blobatar/13.svg",
    "icons/subagents/blobatar/14.svg",
    "icons/subagents/blobatar/15.svg",
    "icons/subagents/blobatar/16.svg",
    "icons/subagents/blobatar/17.svg",
    "icons/subagents/blobatar/18.svg",
    "icons/subagents/blobatar/19.svg",
    "icons/subagents/blobatar/20.svg",
    "icons/subagents/blobatar/21.svg",
    "icons/subagents/blobatar/22.svg",
    "icons/subagents/blobatar/23.svg",
    "icons/subagents/blobatar/24.svg",
    "icons/subagents/blobatar/25.svg",
    "icons/subagents/blobatar/26.svg",
    "icons/subagents/blobatar/27.svg",
];

pub(crate) fn subagent_avatar_index(seed: &str) -> usize {
    let hash = seed.encode_utf16().fold(0_u64, |hash, unit| {
        (hash * 31 + u64::from(unit)) % AVATAR_MODULUS
    });
    hash as usize % BLOBATAR_AVATARS.len()
}

pub(crate) fn blobatar_subagent_avatar_path(seed: &str) -> &'static str {
    BLOBATAR_AVATARS[subagent_avatar_index(seed)]
}

#[cfg(test)]
mod tests {
    use super::{blobatar_subagent_avatar_path, subagent_avatar_index};

    #[test]
    fn subagent_avatar_hash_remains_stable() {
        for (seed, expected) in [
            ("", 0),
            ("subagent-1", 23),
            ("chat--sub--call_task--sub-1", 3),
            ("review-omp-ui", 15),
            ("💡", 8),
            ("éclair", 15),
        ] {
            assert_eq!(subagent_avatar_index(seed), expected, "{seed}");
        }
    }

    #[test]
    fn blobatar_subagent_avatar_selects_a_stable_variant() {
        assert_eq!(
            blobatar_subagent_avatar_path("subagent-1"),
            "icons/subagents/blobatar/23.svg"
        );
        assert_eq!(subagent_avatar_index("subagent-1"), 23);
    }

    #[test]
    fn subagent_avatar_ids_distribute_across_variants() {
        let variants = (0..32)
            .map(|index| subagent_avatar_index(&format!("subagent-{index}")))
            .collect::<std::collections::HashSet<_>>();
        assert!(variants.len() > 8, "only {} variants", variants.len());
    }
}
