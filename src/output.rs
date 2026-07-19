use comfy_table::Table;
use serde_json::Value;

pub fn print_json(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

/// Render a device list as a table. Accepts either a bare array or an object
/// with a `devices` array.
pub fn print_devices_table(v: &Value) {
    let rows = v
        .get("devices")
        .and_then(Value::as_array)
        .or_else(|| v.as_array());

    let mut table = Table::new();
    table.set_header(vec!["Serial", "Name", "Platform", "Last Seen"]);
    if let Some(rows) = rows {
        for d in rows {
            table.add_row(vec![
                field(d, "serialNumber"),
                field(d, "deviceName"),
                field(d, "platform"),
                field(d, "lastSeen"),
            ]);
        }
    }
    println!("{table}");
    if let Some(total) = v.get("total") {
        println!("total: {total}");
    }
}

/// Render an event list as a table. Accepts either a bare array or an object
/// with an `events` array.
pub fn print_events_table(v: &Value) {
    let rows = v
        .get("events")
        .and_then(Value::as_array)
        .or_else(|| v.as_array());

    let mut table = Table::new();
    table.set_header(vec!["Time", "Serial", "Kind", "Message"]);
    if let Some(rows) = rows {
        for e in rows {
            let message = field(e, "message");
            let clipped = if message.chars().count() > 80 {
                let mut s: String = message.chars().take(77).collect();
                s.push_str("...");
                s
            } else {
                message
            };
            table.add_row(vec![
                first_field(e, &["timestamp", "ts", "createdAt"]),
                first_field(e, &["serialNumber", "device", "deviceId"]),
                first_field(e, &["kind", "eventType", "status"]),
                clipped,
            ]);
        }
    }
    println!("{table}");
}

fn field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_string()
}

fn first_field(v: &Value, keys: &[&str]) -> String {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(Value::as_str) {
            return s.to_string();
        }
    }
    "-".to_string()
}
