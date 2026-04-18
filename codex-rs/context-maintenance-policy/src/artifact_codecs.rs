use codex_protocol::error::Result as CodexResult;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use serde_json::json;

use crate::ArtifactKind;

const CONTINUATION_BRIDGE_TAG: &str = "continuation_bridge";
const THREAD_MEMORY_TAG: &str = "thread_memory";
const PRUNE_MANIFEST_TAG: &str = "prune_manifest";
const PRUNE_MANIFEST_SCHEMA: &str = "prune_manifest_v1";

pub fn content_items_to_text(content: &[ContentItem]) -> Option<String> {
    let mut pieces = Vec::new();
    for item in content {
        match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                if !text.is_empty() {
                    pieces.push(text.as_str());
                }
            }
            ContentItem::InputImage { .. } => {}
        }
    }
    if pieces.is_empty() {
        None
    } else {
        Some(pieces.join("\n"))
    }
}

pub fn extract_tagged_payload<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let close_tag = format!("</{tag}>");
    let mut search_start = 0usize;

    while let Some(open_index) = find_open_tag_start(&text[search_start..], tag) {
        let open_index = search_start + open_index;
        let open_tag_end = text[open_index..].find('>')? + open_index + 1;
        if let Some(close_index) = text[open_tag_end..].find(&close_tag) {
            let close_index = open_tag_end + close_index;
            let payload = text[open_tag_end..close_index].trim();
            if !payload.is_empty() {
                return Some(payload);
            }
        }
        search_start = open_index.saturating_add(1);
    }

    None
}

pub fn has_tagged_block(text: &str, tag: &str) -> bool {
    find_open_tag_start(text, tag).is_some() && find_close_tag_start(text, tag).is_some()
}

pub fn tagged_artifact_kind_from_text(text: &str) -> Option<ArtifactKind> {
    if has_tagged_block(text, CONTINUATION_BRIDGE_TAG) {
        return Some(ArtifactKind::ContinuationBridge);
    }
    if has_tagged_block(text, THREAD_MEMORY_TAG) {
        return Some(ArtifactKind::ThreadMemory);
    }
    if has_tagged_block(text, PRUNE_MANIFEST_TAG) {
        return Some(ArtifactKind::PruneManifest);
    }
    None
}

pub fn tagged_artifact_kind(item: &ResponseItem) -> Option<ArtifactKind> {
    let ResponseItem::Message { role, content, .. } = item else {
        return None;
    };
    if role != "developer" {
        return None;
    }
    let text = content_items_to_text(content)?;
    tagged_artifact_kind_from_text(&text)
}

fn find_open_tag_start(text: &str, tag: &str) -> Option<usize> {
    find_tag_start(text, "<", tag, is_open_tag_boundary)
}

fn find_close_tag_start(text: &str, tag: &str) -> Option<usize> {
    find_tag_start(text, "</", tag, is_close_tag_boundary)
}

fn find_tag_start(
    text: &str,
    prefix: &str,
    tag: &str,
    is_valid_boundary: fn(&str) -> bool,
) -> Option<usize> {
    text.match_indices(prefix).find_map(|(idx, _)| {
        let rest = &text[idx + prefix.len()..];
        let after_tag = rest.strip_prefix(tag)?;
        if is_valid_boundary(after_tag) {
            Some(idx)
        } else {
            None
        }
    })
}

fn is_open_tag_boundary(after_tag: &str) -> bool {
    matches!(after_tag.chars().next(), Some('>'))
        || after_tag.chars().next().is_some_and(char::is_whitespace)
}

fn is_close_tag_boundary(after_tag: &str) -> bool {
    matches!(after_tag.chars().next(), Some('>'))
}

pub fn remove_artifact_kind(
    items: Vec<ResponseItem>,
    kind: ArtifactKind,
) -> (Vec<ResponseItem>, usize) {
    let mut removed = 0usize;
    let items = items
        .into_iter()
        .filter_map(|item| {
            if tagged_artifact_kind(&item) == Some(kind) {
                removed = removed.saturating_add(1);
                None
            } else {
                Some(item)
            }
        })
        .collect();
    (items, removed)
}

pub fn prune_superseded_artifacts(items: Vec<ResponseItem>) -> (Vec<ResponseItem>, usize) {
    let mut last_continuation_bridge = None;
    let mut last_thread_memory = None;
    let mut last_prune_manifest = None;
    for (idx, item) in items.iter().enumerate().rev() {
        match tagged_artifact_kind(item) {
            Some(ArtifactKind::ContinuationBridge) if last_continuation_bridge.is_none() => {
                last_continuation_bridge = Some(idx);
            }
            Some(ArtifactKind::ThreadMemory) if last_thread_memory.is_none() => {
                last_thread_memory = Some(idx);
            }
            Some(ArtifactKind::PruneManifest) if last_prune_manifest.is_none() => {
                last_prune_manifest = Some(idx);
            }
            _ => {}
        }
    }

    let mut removed = 0usize;
    let items = items
        .into_iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let keep = match tagged_artifact_kind(&item) {
                Some(ArtifactKind::ContinuationBridge) => Some(idx) == last_continuation_bridge,
                Some(ArtifactKind::ThreadMemory) => Some(idx) == last_thread_memory,
                Some(ArtifactKind::PruneManifest) => Some(idx) == last_prune_manifest,
                _ => true,
            };
            if keep {
                Some(item)
            } else {
                removed = removed.saturating_add(1);
                None
            }
        })
        .collect();
    (items, removed)
}

pub fn build_prune_manifest_item(
    original_len: usize,
    final_len: usize,
    removed_count: usize,
) -> CodexResult<ResponseItem> {
    let manifest = json!({
        "schema": PRUNE_MANIFEST_SCHEMA,
        "removed_item_count": removed_count,
        "original_history_len": original_len,
        "final_history_len": final_len,
    });
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    Ok(ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: format!(
                "<{PRUNE_MANIFEST_TAG} schema=\"{PRUNE_MANIFEST_SCHEMA}\">\n{manifest_json}\n</{PRUNE_MANIFEST_TAG}>"
            ),
        }],
        end_turn: None,
        phase: None,
    })
}

#[cfg(test)]
mod tests {
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;
    use pretty_assertions::assert_eq;

    use super::build_prune_manifest_item;
    use super::content_items_to_text;
    use super::extract_tagged_payload;
    use super::has_tagged_block;
    use super::prune_superseded_artifacts;
    use super::remove_artifact_kind;
    use super::tagged_artifact_kind;
    use super::tagged_artifact_kind_from_text;
    use crate::ArtifactKind;

    #[test]
    fn content_items_to_text_joins_text_and_skips_images() {
        let content = vec![
            ContentItem::InputText {
                text: "input".to_string(),
            },
            ContentItem::InputImage {
                image_url: "ignored".to_string(),
            },
            ContentItem::OutputText {
                text: "output".to_string(),
            },
        ];

        assert_eq!(
            content_items_to_text(&content),
            Some("input\noutput".to_string())
        );
    }

    #[test]
    fn extract_tagged_payload_returns_inner_body() {
        let tagged = "<thread_memory schema=\"odeu_thread_memory_v1\">\n{\"schema\":\"odeu_thread_memory_v1\"}\n</thread_memory>";

        assert_eq!(
            extract_tagged_payload(tagged, "thread_memory"),
            Some("{\"schema\":\"odeu_thread_memory_v1\"}")
        );
    }

    #[test]
    fn extract_tagged_payload_ignores_prefix_sharing_tags_and_finds_later_valid_tag() {
        let tagged =
            "<thread_memory_v2>wrong</thread_memory_v2>\n<thread_memory>right</thread_memory>";

        assert_eq!(
            extract_tagged_payload(tagged, "thread_memory"),
            Some("right")
        );
    }

    #[test]
    fn tagged_block_detection_matches_open_and_close_tags() {
        assert_eq!(
            has_tagged_block(
                "<thread_memory schema=\"x\">payload</thread_memory>",
                "thread_memory",
            ),
            true
        );
        assert_eq!(
            has_tagged_block("<thread_memory>payload", "thread_memory"),
            false
        );
        assert_eq!(
            has_tagged_block("payload</thread_memory>", "thread_memory"),
            false
        );
        assert_eq!(
            has_tagged_block(
                "<thread_memory_v2>payload</thread_memory_v2>",
                "thread_memory",
            ),
            false
        );
    }

    #[test]
    fn tagged_artifact_kind_from_text_ignores_prefix_sharing_tags() {
        assert_eq!(
            tagged_artifact_kind_from_text("<thread_memory_v2>payload</thread_memory_v2>",),
            None
        );
    }

    #[test]
    fn tagged_artifact_kind_detects_known_developer_artifacts() {
        assert_eq!(
            tagged_artifact_kind(&developer_message(
                "<continuation_bridge>payload</continuation_bridge>",
            )),
            Some(ArtifactKind::ContinuationBridge)
        );
        assert_eq!(
            tagged_artifact_kind(&developer_message("<thread_memory>payload</thread_memory>",)),
            Some(ArtifactKind::ThreadMemory)
        );
        assert_eq!(
            tagged_artifact_kind(&developer_message(
                "<prune_manifest>payload</prune_manifest>",
            )),
            Some(ArtifactKind::PruneManifest)
        );
        assert_eq!(tagged_artifact_kind(&user_message("plain")), None);
    }

    #[test]
    fn prune_superseded_artifacts_keeps_latest_per_kind() {
        let items = vec![
            developer_message("<thread_memory>old memory</thread_memory>"),
            developer_message("<continuation_bridge>old bridge</continuation_bridge>"),
            developer_message("<thread_memory>new memory</thread_memory>"),
            developer_message("<prune_manifest>old prune</prune_manifest>"),
            developer_message("<continuation_bridge>new bridge</continuation_bridge>"),
            developer_message("<prune_manifest>new prune</prune_manifest>"),
        ];

        let (items, removed) = prune_superseded_artifacts(items);

        assert_eq!(removed, 3);
        assert_eq!(
            items,
            vec![
                developer_message("<thread_memory>new memory</thread_memory>"),
                developer_message("<continuation_bridge>new bridge</continuation_bridge>"),
                developer_message("<prune_manifest>new prune</prune_manifest>"),
            ]
        );
    }

    #[test]
    fn remove_artifact_kind_removes_only_matching_kind() {
        let items = vec![
            developer_message("<thread_memory>memory</thread_memory>"),
            developer_message("<prune_manifest>prune</prune_manifest>"),
        ];

        let (items, removed) = remove_artifact_kind(items, ArtifactKind::PruneManifest);

        assert_eq!(removed, 1);
        assert_eq!(
            items,
            vec![developer_message("<thread_memory>memory</thread_memory>")]
        );
    }

    #[test]
    fn build_prune_manifest_item_emits_tagged_developer_message() {
        let item =
            build_prune_manifest_item(10, 7, 3).expect("prune manifest item should serialize");

        assert_eq!(
            tagged_artifact_kind(&item),
            Some(ArtifactKind::PruneManifest)
        );
        let ResponseItem::Message { role, content, .. } = item else {
            panic!("expected prune manifest developer message");
        };
        assert_eq!(role, "developer");
        assert_eq!(
            content_items_to_text(&content),
            Some(
                "<prune_manifest schema=\"prune_manifest_v1\">\n{\n  \"final_history_len\": 7,\n  \"original_history_len\": 10,\n  \"removed_item_count\": 3,\n  \"schema\": \"prune_manifest_v1\"\n}\n</prune_manifest>"
                    .to_string()
            )
        );
    }

    fn developer_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "developer".to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    fn user_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }
}
