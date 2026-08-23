use crate::theme::Appearance;

const CODEX_AVATAR_MODULUS: u64 = 2_147_483_647;
const CODEX_AVATARS: [[&str; 2]; 28] = [
    [
        "icons/subagents/codex/00-dark.svg",
        "icons/subagents/codex/00-light.svg",
    ],
    [
        "icons/subagents/codex/01-dark.svg",
        "icons/subagents/codex/01-light.svg",
    ],
    [
        "icons/subagents/codex/02-dark.svg",
        "icons/subagents/codex/02-light.svg",
    ],
    [
        "icons/subagents/codex/03-dark.svg",
        "icons/subagents/codex/03-light.svg",
    ],
    [
        "icons/subagents/codex/04-dark.svg",
        "icons/subagents/codex/04-light.svg",
    ],
    [
        "icons/subagents/codex/05-dark.svg",
        "icons/subagents/codex/05-light.svg",
    ],
    [
        "icons/subagents/codex/06-dark.svg",
        "icons/subagents/codex/06-light.svg",
    ],
    [
        "icons/subagents/codex/07-dark.svg",
        "icons/subagents/codex/07-light.svg",
    ],
    [
        "icons/subagents/codex/08-dark.svg",
        "icons/subagents/codex/08-light.svg",
    ],
    [
        "icons/subagents/codex/09-dark.svg",
        "icons/subagents/codex/09-light.svg",
    ],
    [
        "icons/subagents/codex/10-dark.svg",
        "icons/subagents/codex/10-light.svg",
    ],
    [
        "icons/subagents/codex/11-dark.svg",
        "icons/subagents/codex/11-light.svg",
    ],
    [
        "icons/subagents/codex/12-dark.svg",
        "icons/subagents/codex/12-light.svg",
    ],
    [
        "icons/subagents/codex/13-dark.svg",
        "icons/subagents/codex/13-light.svg",
    ],
    [
        "icons/subagents/codex/14-dark.svg",
        "icons/subagents/codex/14-light.svg",
    ],
    [
        "icons/subagents/codex/15-dark.svg",
        "icons/subagents/codex/15-light.svg",
    ],
    [
        "icons/subagents/codex/16-dark.svg",
        "icons/subagents/codex/16-light.svg",
    ],
    [
        "icons/subagents/codex/17-dark.svg",
        "icons/subagents/codex/17-light.svg",
    ],
    [
        "icons/subagents/codex/18-dark.svg",
        "icons/subagents/codex/18-light.svg",
    ],
    [
        "icons/subagents/codex/19-dark.svg",
        "icons/subagents/codex/19-light.svg",
    ],
    [
        "icons/subagents/codex/20-dark.svg",
        "icons/subagents/codex/20-light.svg",
    ],
    [
        "icons/subagents/codex/21-dark.svg",
        "icons/subagents/codex/21-light.svg",
    ],
    [
        "icons/subagents/codex/22-dark.svg",
        "icons/subagents/codex/22-light.svg",
    ],
    [
        "icons/subagents/codex/23-dark.svg",
        "icons/subagents/codex/23-light.svg",
    ],
    [
        "icons/subagents/codex/24-dark.svg",
        "icons/subagents/codex/24-light.svg",
    ],
    [
        "icons/subagents/codex/25-dark.svg",
        "icons/subagents/codex/25-light.svg",
    ],
    [
        "icons/subagents/codex/26-dark.svg",
        "icons/subagents/codex/26-light.svg",
    ],
    [
        "icons/subagents/codex/27-dark.svg",
        "icons/subagents/codex/27-light.svg",
    ],
];

pub(crate) fn codex_subagent_avatar_index(seed: &str) -> usize {
    let hash = seed.encode_utf16().fold(0_u64, |hash, unit| {
        (hash * 31 + u64::from(unit)) % CODEX_AVATAR_MODULUS
    });
    hash as usize % CODEX_AVATARS.len()
}

pub(crate) fn codex_subagent_avatar_path(seed: &str, appearance: Appearance) -> &'static str {
    let pair = CODEX_AVATARS[codex_subagent_avatar_index(seed)];
    pair[usize::from(appearance.is_light())]
}

#[cfg(test)]
mod tests {
    use crate::theme::Appearance;

    use super::{codex_subagent_avatar_index, codex_subagent_avatar_path};

    #[test]
    fn codex_subagent_avatar_hash_matches_desktop_vectors() {
        for (seed, expected) in [
            ("", 0),
            ("subagent-1", 23),
            ("chat--sub--call_task--sub-1", 3),
            ("review-omp-ui", 15),
            ("💡", 8),
            ("éclair", 15),
        ] {
            assert_eq!(codex_subagent_avatar_index(seed), expected, "{seed}");
        }
    }

    #[test]
    fn codex_subagent_avatar_selects_a_stable_appearance_pair() {
        assert_eq!(
            codex_subagent_avatar_path("subagent-1", Appearance::Dark),
            "icons/subagents/codex/23-dark.svg"
        );
        assert_eq!(
            codex_subagent_avatar_path("subagent-1", Appearance::Light),
            "icons/subagents/codex/23-light.svg"
        );
        assert_eq!(codex_subagent_avatar_index("subagent-1"), 23);
    }

    #[test]
    fn codex_subagent_avatar_ids_distribute_across_variants() {
        let variants = (0..32)
            .map(|index| codex_subagent_avatar_index(&format!("subagent-{index}")))
            .collect::<std::collections::HashSet<_>>();
        assert!(variants.len() > 8, "only {} variants", variants.len());
    }
}
