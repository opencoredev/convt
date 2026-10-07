//! Explorer commands. Only the installed CLI probes engines; the DLL never
//! loads codecs into Explorer or duplicates the registry's routing policy.
#[cfg(windows)]
mod windows;

/// Preserve the first file's menu order and offer only targets all files share.
pub fn common_targets(lists: &[Vec<String>]) -> Vec<String> {
    lists.first().map_or_else(Vec::new, |first| {
        first
            .iter()
            .filter(|target| lists.iter().all(|list| list.contains(target)))
            .cloned()
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_selection_preserves_menu_order_and_requires_every_file() {
        let lists = vec![
            vec!["webp".into(), "jpeg".into(), "pdf".into()],
            vec!["pdf".into(), "webp".into()],
        ];
        assert_eq!(common_targets(&lists), ["webp", "pdf"]);
        assert!(common_targets(&[lists[0].clone(), vec![]]).is_empty());
        assert!(common_targets(&[]).is_empty());
    }
}
