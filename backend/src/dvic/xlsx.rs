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
mod tests {
    use super::*;
    use std::io::Write;
    fn workbook(headers: &[&str], rows: &[Inspection], dimension: &str) -> Vec<u8> {
        let escape = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        };
        let mut xml = format!(
            "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><dimension ref=\"{dimension}\"/><sheetData>"
        );
        let data = std::iter::once(headers.iter().map(|h| (*h).to_owned()).collect::<Vec<_>>())
            .chain(rows.iter().map(|r| {
                let v = serde_json::to_value(r).unwrap();
                headers
                    .iter()
                    .map(|h| {
                        v[*h]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v[*h].to_string())
                    })
                    .collect()
            }));
        for (index, row) in data.enumerate() {
            xml.push_str(&format!("<row r=\"{}\">", index + 1));
            for (col, value) in row.iter().enumerate() {
                xml.push_str(&format!(
                    "<c r=\"{}{}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                    (b'A' + col as u8) as char,
                    index + 1,
                    escape(value)
                ));
            }
            xml.push_str("</row>");
        }
        xml.push_str("</sheetData></worksheet>");
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, contents) in [
            (
                "_rels/.rels",
                "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Target=\"xl/workbook.xml\" \
                 Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\"/>\
                 </Relationships>",
            ),
            (
                "[Content_Types].xml",
                "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Override \
                    PartName=\"/xl/workbook.xml\" \
                    ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
                    <Override PartName=\"/xl/worksheets/sheet1.xml\" \
                    ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>",
            ),
            (
                "xl/workbook.xml",
                "<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" \
                    xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet \
                    name=\"DVIC Detail\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>",
            ),
            (
                "xl/_rels/workbook.xml.rels",
                "<Relationships \
                    xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" \
                    Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" \
                    Target=\"worksheets/sheet1.xml\"/></Relationships>",
            ),
            ("xl/worksheets/sheet1.xml", xml.as_str()),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    fn row() -> Inspection {
        let request = Request {
            collection: Collection::Dvic,
            station: "TST1".into(),
            weeks: vec!["2026-W39".into()],
        };
        fixture(&request)
            .unwrap()
            .reports
            .remove(0)
            .rows
            .unwrap()
            .remove(0)
    }
    #[test]
    fn reordered_columns_and_exaggerated_dimensions_do_not_change_records_or_allocate_a_grid() {
        let mut headers = HEADERS;
        headers.reverse();
        let mut row = row();
        row.transporter_name = "Example & Driver".into();
        let bytes = workbook(&headers, &[row.clone()], "A1:XFD1048576");
        assert_eq!(parse(&bytes, "FXTR", "TST1").unwrap(), vec![row]);
        assert!(parse(&bytes, "OTHER", "TST1").is_err());
        assert!(parse(&bytes, "FXTR", "TST2").is_err());
        assert!(
            parse(&workbook(&HEADERS, &[], "A1:L1"), "FXTR", "TST1")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn malformed_workbooks_columns_values_and_duplicate_identities_fail_the_whole_file() {
        assert!(parse(b"not an Excel workbook", "FXTR", "TST1").is_err());
        let mut headers = HEADERS;
        headers[11] = "unexpected_duration";
        assert!(parse(&workbook(&headers, &[], "A1:L1"), "FXTR", "TST1").is_err());
        let original = row();
        let mut changed = original.clone();
        changed.transporter_name = "New Name".into();
        assert!(
            parse(
                &workbook(&HEADERS, &[original.clone(), changed], "A1:L3"),
                "FXTR",
                "TST1"
            )
            .is_err()
        );
        let mut bad = original.clone();
        bad.duration = 1.0;
        assert!(parse(&workbook(&HEADERS, &[bad], "A1:L2"), "FXTR", "TST1").is_err());
        let mut bad = original;
        bad.fleet_type = "UNKNOWN".into();
        assert!(parse(&workbook(&HEADERS, &[bad], "A1:L2"), "FXTR", "TST1").is_err());
    }
}
