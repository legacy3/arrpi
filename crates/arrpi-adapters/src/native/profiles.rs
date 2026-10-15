//! Decode enabled native quality groups before marking a profile as non-4K.
use serde_json::Value;

pub(crate) fn safe_qualities(profile: &Value) -> bool {
    let Some(rows) = profile.get("items").and_then(Value::as_array) else {
        return false;
    };
    let mut pending = rows.iter().map(|node| (node, None, 0)).collect::<Vec<_>>();
    let mut count = 0;
    let mut enabled_count = 0;

    while let Some((node, parent_allowed, depth)) = pending.pop() {
        count += 1;

        if count > 4000 || depth > 4 {
            return false;
        }

        let Some(allowed) = node.get("allowed").and_then(Value::as_bool) else {
            return false;
        };
        let enabled = parent_allowed.unwrap_or(allowed);

        if let Some(quality) = node.get("quality").filter(|quality| quality.is_object()) {
            if enabled {
                if !safe_resolution(quality) {
                    return false;
                }

                enabled_count += 1;
            }
        } else if let Some(children) = node
            .get("items")
            .and_then(Value::as_array)
            .filter(|rows| !rows.is_empty())
        {
            pending.extend(
                children
                    .iter()
                    .map(|child| (child, Some(enabled), depth + 1)),
            );
        } else {
            return false;
        }
    }

    enabled_count > 0
}

fn safe_resolution(quality: &Value) -> bool {
    quality["resolution"]
        .as_u64()
        .is_some_and(|value| (1..=1080).contains(&value))
        // Radarr's native SD DVD definition has resolution 0, unlike Unknown.
        || (quality["resolution"] == 0
            && quality["id"] == 2
            && quality["name"] == "DVD"
            && quality["source"] == "dvd"
            && quality["modifier"] == "none")
}
