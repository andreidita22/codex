mod history;
mod normalize;
pub(crate) mod updates;

use codex_protocol::models::ResponseItem;

pub(crate) use history::ContextManager;
pub(crate) use history::TotalTokenUsageBreakdown;
pub(crate) use history::estimate_response_item_model_visible_bytes;
pub(crate) use history::is_codex_generated_item;
pub(crate) use history::is_user_turn_boundary;

pub(crate) fn remove_corresponding_for(items: &mut Vec<ResponseItem>, item: &ResponseItem) {
    normalize::remove_corresponding_for(items, item);
}
