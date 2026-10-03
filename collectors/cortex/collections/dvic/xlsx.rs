//! Bounded XLSX decoding. Stream cells so sparse or inflated sheet dimensions do
//! not cause a rectangular allocation. Never evaluate formulas or external links.
use super::*;
use calamine::{DataRef, Reader, Xlsx};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};

const HEADERS: [&str; 12] = [
    "start_date",
    "dsp",
    "station",
    "transporter_id",
    "transporter_name",
    "vin",
    "fleet_type",
    "inspection_type",
    "inspection_status",
    "start_time",
    "end_time",
    "duration",
];
const EXPANDED_LIMIT: u64 = 32 * 1024 * 1024;

pub fn parse(bytes: &[u8], dsp: &str, station: &str) -> Result<Vec<Inspection>> {
    let invalid = || Error::new("dvic_workbook_invalid", 502);
    ensure(bytes.len() <= MAX_FILE_BYTES, "dvic_source_too_large", 502)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid())?;
    ensure(archive.len() <= 128, "dvic_source_too_large", 502)?;
    let mut remaining = EXPANDED_LIMIT;
    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|_| invalid())?;
        ensure(file.size() <= remaining, "dvic_source_too_large", 502)?;
        let read = std::io::copy(&mut file.take(remaining + 1), &mut std::io::sink())
            .map_err(|_| invalid())?;
        ensure(read <= remaining, "dvic_source_too_large", 502)?;
        remaining -= read;
    }
    let mut workbook = Xlsx::new(Cursor::new(bytes)).map_err(|_| invalid())?;
    let mut reader = workbook
        .worksheet_cells_reader("DVIC Detail")
        .map_err(|_| invalid())?;
    let mut rows: BTreeMap<u32, BTreeMap<u32, String>> = BTreeMap::new();
    while let Some(cell) = reader.next_cell().map_err(|_| invalid())? {
        let (row, col) = cell.get_position();
        ensure(
            row <= MAX_ROWS as u32 && col < HEADERS.len() as u32,
            "dvic_source_too_large",
            502,
        )?;
        let value = match cell.get_value() {
            DataRef::String(s) => s.clone(),
            DataRef::SharedString(s) => (*s).to_owned(),
            DataRef::Int(n) => n.to_string(),
            DataRef::Float(n) if n.is_finite() => n.to_string(),
            DataRef::Empty => String::new(),
            _ => return Err(invalid()),
        };
        ensure(value.len() <= 512, "dvic_source_too_large", 502)?;
        ensure(
            rows.entry(row).or_default().insert(col, value).is_none(),
            "dvic_workbook_invalid",
            502,
        )?;
    }
    let headers = rows.remove(&0).ok_or_else(invalid)?;
    ensure(
        headers.len() == HEADERS.len()
            && HEADERS
                .iter()
                .all(|h| headers.values().filter(|v| v.as_str() == *h).count() == 1),
        "dvic_columns_changed",
        502,
    )?;
    let mut result = Vec::new();
    let mut keys = HashSet::new();
    for row in rows
        .values()
        .filter(|row| row.values().any(|v| !v.is_empty()))
    {
        let mut object = serde_json::Map::new();
        for (column, header) in &headers {
            let text = row.get(column).ok_or_else(invalid)?;
            let value = if header == "duration" {
                let number: f64 = text.parse().map_err(|_| invalid())?;
                ensure(number.is_finite(), "dvic_duration_invalid", 502)?;
                json!(number)
            } else {
                json!(text)
            };
            object.insert(header.clone(), value);
        }
        let inspection: Inspection =
            serde_json::from_value(Value::Object(object)).map_err(|_| invalid())?;
        inspection.validate(dsp, station)?;
        ensure(
            keys.insert(inspection.key()),
            "dvic_duplicate_inspection",
            502,
        )?;
        result.push(inspection);
    }
    Ok(result)
}

#[cfg(test)]
#[path = "../../tests/backend/collections/dvic/xlsx.rs"]
mod tests;
