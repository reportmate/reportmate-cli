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
    table.set_header(vec!["Serial", "Name", "Last Seen"]);
    if let Some(rows) = rows {
        for d in rows {
            table.add_row(vec![
                field(d, "serialNumber"),
                field(d, "deviceName"),
                field(d, "lastSeen"),
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
