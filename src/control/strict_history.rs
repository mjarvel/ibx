//! Opt-in validation of finite historical bar responses.
//!
//! Unsupported syntax and unknown information fail explicitly.
//! Request notices and errors travel separately; callers must retain them.

/// A validated bar with original timestamp text and optional statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct StrictHistoricalBar {
    /// Original daily or intraday timestamp, without conversion.
    pub time: String,
    /// Original optional bar end timestamp, without inferred response coverage.
    pub end_time: Option<String>,
    /// Finite opening price; negative prices are preserved.
    pub open: f64,
    /// Finite high price.
    pub high: f64,
    /// Finite low price.
    pub low: f64,
    /// Finite closing price.
    pub close: f64,
    /// Nonnegative volume; absent, empty and the unset sentinel become None.
    pub volume: Option<i64>,
    /// Finite weighted average, preserved without request-dependent interpretation.
    pub wap: Option<f64>,
    /// Optional finite time average, distinct from the weighted average.
    pub time_average: Option<f64>,
    /// Nonnegative trade count; absent, empty and the unset sentinel become None.
    pub count: Option<u32>,
}

/// One validated response, without inferred coverage or timestamp conversion.
#[derive(Debug, Clone, PartialEq)]
pub struct StrictHistoricalResponse {
    /// Original nonempty query identifier.
    pub query_id: String,
    /// Original timezone text, or an empty string when absent.
    pub timezone: String,
    /// Validated bars in their original order.
    pub bars: Vec<StrictHistoricalBar>,
    /// Whether this response explicitly identifies itself as the final response.
    /// This does not prove requested coverage or the absence of separate errors.
    pub is_complete: bool,
}

/// Parsing failures without echoing response content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrictHistoryError {
    /// Input bytes exceed the caller's limit.
    ByteLimit,
    /// Bar count exceeds the caller's limit.
    RowLimit,
    /// Input structure is malformed or uses unsupported syntax.
    MalformedXml,
    /// Information outside the accepted response is present.
    UnexpectedField,
    /// A required value is missing or empty.
    MissingField(&'static str),
    /// A value occurs more than once.
    DuplicateField(&'static str),
    /// A value has an invalid representation or range.
    InvalidField(&'static str),
}

impl std::fmt::Display for StrictHistoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "strict historical response: {self:?}")
    }
}
impl std::error::Error for StrictHistoryError {}

type ParseResult<T> = Result<T, StrictHistoryError>;

/// Parse one finite historical bar response with explicit byte and bar limits.
/// Limits apply before owned response allocations; structural validation uses a
/// fixed-depth borrowed stack. The caller must also bound cumulative responses and
/// separately delivered notices, and must not publish completion on parser error.
/// Missing/empty statistics are absent; volume/count -1 are absent; WAP -1 remains
/// a raw price. Raw time text and timezone are not converted or manufactured.
pub fn parse_bar_response_strict(
    xml: &str,
    max_rows: usize,
    max_bytes: usize,
) -> Result<StrictHistoricalResponse, StrictHistoryError> {
    if xml.len() > max_bytes {
        return Err(StrictHistoryError::ByteLimit);
    }
    let xml = strip_declaration(xml)?;
    validate_xml(xml, max_rows)?;
    let mut document = xml;
    let root = next_element(&mut document)?.ok_or(StrictHistoryError::MalformedXml)?;
    if root.name != "ResultSetBar" || !document.trim().is_empty() {
        return Err(StrictHistoryError::MalformedXml);
    }
    let mut query_id = None;
    let mut timezone = None;
    let mut is_complete = None;
    let mut events = None;
    let mut fields = root.body;
    while let Some(field) = next_element(&mut fields)? {
        match field.name {
            "id" => set_once(&mut query_id, nonempty(field, "id")?, "id")?,
            "tz" => set_once(&mut timezone, scalar(field)?, "tz")?,
            "eoq" => {
                let complete = match scalar(field)?.trim() {
                    "true" => true,
                    "false" => false,
                    _ => return Err(StrictHistoryError::InvalidField("eoq")),
                };
                set_once(&mut is_complete, complete, "eoq")?;
            }
            "Events" => set_once(&mut events, field.body, "Events")?,
            _ => return Err(StrictHistoryError::UnexpectedField),
        }
    }
    let query_id = query_id.ok_or(StrictHistoryError::MissingField("id"))?;
    let is_complete = is_complete.ok_or(StrictHistoryError::MissingField("eoq"))?;
    let mut events = events.ok_or(StrictHistoryError::MissingField("Events"))?;
    let mut bars = Vec::new();
    while let Some(event) = next_element(&mut events)? {
        match event.name {
            "Bar" => {
                // The preflight already checked all rows, before any owned strings.
                if bars.len() >= max_rows {
                    return Err(StrictHistoryError::RowLimit);
                }
                bars.push(parse_bar(event.body)?);
            }
            "Open" | "Close" => validate_session_event(event.body)?,
            _ => return Err(StrictHistoryError::UnexpectedField),
        }
    }
    Ok(StrictHistoricalResponse {
        query_id: query_id.to_owned(),
        timezone: timezone.unwrap_or("").to_owned(),
        bars,
        is_complete,
    })
}

fn set_once<T>(target: &mut Option<T>, value: T, field: &'static str) -> ParseResult<()> {
    if target.is_some() {
        return Err(StrictHistoryError::DuplicateField(field));
    }
    *target = Some(value);
    Ok(())
}

#[derive(Clone, Copy)]
struct Element<'a> {
    name: &'a str,
    body: &'a str,
}

fn strip_declaration(xml: &str) -> ParseResult<&str> {
    let xml = xml.trim();
    if let Some(rest) = xml.strip_prefix("<?xml ") {
        let end = rest.find("?>").ok_or(StrictHistoryError::MalformedXml)?;
        let declaration = rest[..end].trim();
        if !matches!(
            declaration,
            "version=\"1.0\"" | "version=\"1.0\" encoding=\"UTF-8\""
        ) {
            return Err(StrictHistoryError::MalformedXml);
        }
        Ok(rest[end + 2..].trim())
    } else {
        Ok(xml)
    }
}

/// Validate every open/close pair without allocating a DOM or growing a stack.
fn validate_xml(xml: &str, max_rows: usize) -> ParseResult<()> {
    if xml.chars().any(|c| {
        (c.is_control() && !matches!(c, '\t' | '\n' | '\r')) || matches!(c, '\u{fffe}' | '\u{ffff}')
    }) {
        return Err(StrictHistoryError::MalformedXml);
    }
    let mut stack = [""; 16];
    let mut depth = 0;
    let mut rows = 0usize;
    let mut rest = xml;
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            if rest.trim().is_empty() {
                break;
            }
            return Err(StrictHistoryError::MalformedXml);
        };
        // Entities/CDATA/comments/attributes are outside this native simple schema;
        // rejecting them prevents malformed or ambiguous text from becoming proof.
        if rest[..open].contains(['&', '>']) || (depth == 0 && !rest[..open].trim().is_empty()) {
            return Err(StrictHistoryError::MalformedXml);
        }
        rest = &rest[open + 1..];
        let end = rest.find('>').ok_or(StrictHistoryError::MalformedXml)?;
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            if depth == 0 || stack[depth - 1] != name {
                return Err(StrictHistoryError::MalformedXml);
            }
            depth -= 1;
        } else {
            let name = tag.strip_suffix('/').unwrap_or(tag);
            if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return Err(StrictHistoryError::MalformedXml);
            }
            if name == "Bar" {
                rows = rows.checked_add(1).ok_or(StrictHistoryError::RowLimit)?;
                if rows > max_rows {
                    return Err(StrictHistoryError::RowLimit);
                }
            }
            if !tag.ends_with('/') {
                if depth == stack.len() {
                    return Err(StrictHistoryError::MalformedXml);
                }
                stack[depth] = name;
                depth += 1;
            }
        }
    }
    if depth != 0 || xml.is_empty() {
        return Err(StrictHistoryError::MalformedXml);
    }
    Ok(())
}

/// Walk validated borrowed children; never search through nested unrelated fields.
fn next_element<'a>(input: &mut &'a str) -> ParseResult<Option<Element<'a>>> {
    let text = input.trim_start();
    if text.is_empty() {
        *input = text;
        return Ok(None);
    }
    let text = text
        .strip_prefix('<')
        .ok_or(StrictHistoryError::MalformedXml)?;
    let end = text.find('>').ok_or(StrictHistoryError::MalformedXml)?;
    let tag = &text[..end];
    let mut rest = &text[end + 1..];
    if let Some(name) = tag.strip_suffix('/') {
        *input = rest;
        return Ok(Some(Element { name, body: "" }));
    }
    let body = rest;
    let mut depth = 1usize;
    loop {
        let open = rest.find('<').ok_or(StrictHistoryError::MalformedXml)?;
        let tail = &rest[open + 1..];
        let end = tail.find('>').ok_or(StrictHistoryError::MalformedXml)?;
        let next_tag = &tail[..end];
        if next_tag.starts_with('/') {
            depth -= 1;
            if depth == 0 {
                *input = &tail[end + 1..];
                let body_len = body.len() - rest.len() + open;
                return Ok(Some(Element {
                    name: tag,
                    body: &body[..body_len],
                }));
            }
        } else if !next_tag.ends_with('/') {
            depth += 1;
        }
        rest = &tail[end + 1..];
    }
}

fn scalar(field: Element<'_>) -> ParseResult<&str> {
    if field.body.contains('<') {
        return Err(StrictHistoryError::UnexpectedField);
    }
    Ok(field.body)
}

fn nonempty<'a>(field: Element<'a>, name: &'static str) -> ParseResult<&'a str> {
    let value = scalar(field)?;
    if value.trim().is_empty() {
        return Err(StrictHistoryError::MissingField(name));
    }
    Ok(value)
}

fn finite(text: &str, field: &'static str) -> ParseResult<f64> {
    let value: f64 = text
        .trim()
        .parse()
        .map_err(|_| StrictHistoryError::InvalidField(field))?;
    if !value.is_finite() {
        return Err(StrictHistoryError::InvalidField(field));
    }
    Ok(value)
}

fn parse_bar(body: &str) -> ParseResult<StrictHistoricalBar> {
    let mut values = [None; 10];
    let names = [
        "time",
        "open",
        "high",
        "low",
        "close",
        "volume",
        "weightedAvg",
        "count",
        "endTime",
        "timeAvg",
    ];
    let mut fields = body;
    while let Some(field) = next_element(&mut fields)? {
        let index = names
            .iter()
            .position(|name| *name == field.name)
            .ok_or(StrictHistoryError::UnexpectedField)?;
        set_once(&mut values[index], scalar(field)?, names[index])?;
    }
    let required = |index: usize| -> ParseResult<&str> {
        values[index]
            .filter(|v| !v.trim().is_empty())
            .ok_or(StrictHistoryError::MissingField(names[index]))
    };
    let optional = |index: usize| values[index].filter(|v| !v.trim().is_empty());
    let volume = optional(5)
        .map(|text| {
            let value: i64 = text
                .trim()
                .parse()
                .map_err(|_| StrictHistoryError::InvalidField("volume"))?;
            match value {
                -1 => Ok(None),
                0.. => Ok(Some(value)),
                _ => Err(StrictHistoryError::InvalidField("volume")),
            }
        })
        .transpose()?
        .flatten();
    let count = optional(7)
        .map(|text| {
            let value: i64 = text
                .trim()
                .parse()
                .map_err(|_| StrictHistoryError::InvalidField("count"))?;
            if value == -1 {
                return Ok(None);
            }
            u32::try_from(value)
                .map(Some)
                .map_err(|_| StrictHistoryError::InvalidField("count"))
        })
        .transpose()?
        .flatten();
    Ok(StrictHistoricalBar {
        time: required(0)?.to_owned(),
        end_time: optional(8).map(str::to_owned),
        open: finite(required(1)?, "open")?,
        high: finite(required(2)?, "high")?,
        low: finite(required(3)?, "low")?,
        close: finite(required(4)?, "close")?,
        volume,
        wap: optional(6)
            .map(|text| finite(text, "weightedAvg"))
            .transpose()?,
        time_average: optional(9)
            .map(|text| finite(text, "timeAvg"))
            .transpose()?,
        count,
    })
}

fn validate_session_event(body: &str) -> ParseResult<()> {
    let mut fields = body;
    let mut time = None;
    let mut date = None;
    while let Some(field) = next_element(&mut fields)? {
        match field.name {
            "time" => set_once(&mut time, nonempty(field, "time")?, "time")?,
            "refDate" => set_once(&mut date, nonempty(field, "refDate")?, "refDate")?,
            _ => return Err(StrictHistoryError::UnexpectedField),
        }
    }
    time.ok_or(StrictHistoryError::MissingField("time"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(time: &str, statistics: &str) -> String {
        format!(
            "<Bar><time>{time}</time><open>-1.123456789</open><high>0</high><low>-2</low><close>-1</close>{statistics}</Bar>"
        )
    }
    fn frame(events: &str, complete: &str) -> String {
        format!(
            "<ResultSetBar><id>hist_7</id><tz>US/Eastern</tz><eoq>{complete}</eoq><Events>{events}</Events></ResultSetBar>"
        )
    }
    fn parse(xml: &str) -> Result<StrictHistoricalResponse, StrictHistoryError> {
        parse_bar_response_strict(xml, 10, 10_000)
    }

    #[test]
    fn strict_history_preserves_raw_daily_intraday_timezone_and_finite_negative_prices() {
        let xml = frame(
            &(bar("20261006", "") + &bar("20261006-14:30:00", "")),
            "true",
        );
        let response = parse(&xml).unwrap();
        assert_eq!(response.query_id, "hist_7");
        assert_eq!(response.timezone, "US/Eastern");
        assert!(response.is_complete);
        assert_eq!(response.bars[0].time, "20261006");
        assert_eq!(response.bars[1].time, "20261006-14:30:00");
        assert_eq!(response.bars[0].open, -1.123456789);
        assert_eq!(response.bars[0].high, 0.0);
        assert_eq!(response.bars[0].low, -2.0);
        assert_eq!(response.bars[0].close, -1.0);
    }

    #[test]
    fn strict_history_distinguishes_absent_empty_zero_and_unset_statistics() {
        for stats in [
            "",
            "<volume/><count></count><weightedAvg/>",
            "<volume>-1</volume><count>-1</count>",
        ] {
            let response = parse(&frame(&bar("20261006", stats), "false")).unwrap();
            assert_eq!(response.bars[0].volume, None);
            assert_eq!(response.bars[0].count, None);
            assert_eq!(response.bars[0].wap, None);
            assert!(!response.is_complete);
        }
        let response = parse(&frame(
            &bar(
                "20261006",
                "<volume>0</volume><count>0</count><weightedAvg>0</weightedAvg>",
            ),
            "true",
        ))
        .unwrap();
        assert_eq!(response.bars[0].volume, Some(0));
        assert_eq!(response.bars[0].count, Some(0));
        assert_eq!(response.bars[0].wap, Some(0.0));
        let response = parse(&frame(
            &bar("20261006", "<weightedAvg>-1</weightedAvg>"),
            "true",
        ))
        .unwrap();
        assert_eq!(response.bars[0].wap, Some(-1.0));
    }

    #[test]
    fn strict_history_rejects_missing_malformed_nonfinite_required_prices() {
        for name in ["open", "high", "low", "close"] {
            let xml = frame(&bar("20261006", ""), "true");
            let start = xml.find(&format!("<{name}>")).unwrap();
            let end = xml.find(&format!("</{name}>")).unwrap() + name.len() + 3;
            for replacement in [
                String::new(),
                format!("<{name}></{name}>"),
                format!("<{name}>bad</{name}>"),
                format!("<{name}>NaN</{name}>"),
                format!("<{name}>inf</{name}>"),
            ] {
                let mut damaged = xml.clone();
                damaged.replace_range(start..end, &replacement);
                assert!(parse(&damaged).is_err(), "{name}: {replacement}");
            }
        }
        assert!(parse(&frame(&bar("", ""), "true")).is_err());
    }

    #[test]
    fn strict_history_rejects_invalid_statistics_and_integer_overflow() {
        for stats in [
            "<volume>-2</volume>",
            "<volume>0.5</volume>",
            "<volume>9223372036854775808</volume>",
            "<count>-2</count>",
            "<count>4294967296</count>",
            "<count>x</count>",
            "<weightedAvg>NaN</weightedAvg>",
            "<weightedAvg>inf</weightedAvg>",
        ] {
            assert!(
                parse(&frame(&bar("20261006", stats), "true")).is_err(),
                "{stats}"
            );
        }
    }

    #[test]
    fn strict_history_rejects_damage_after_valid_row_instead_of_successful_partial_result() {
        let xml = frame(&bar("20261006", ""), "true");
        for cut in [
            xml.len() - 1,
            xml.find("</Bar>").unwrap(),
            xml.find("</Events>").unwrap(),
        ] {
            assert_eq!(
                parse(&xml[..cut]).unwrap_err(),
                StrictHistoryError::MalformedXml
            );
        }
        assert!(parse(&(xml.clone() + "<unfinished>")).is_err());
        assert!(parse(&xml.replace("</low>", "</high>")).is_err());
        assert!(parse(&frame(&(bar("20261006", "") + "<Bar>"), "true")).is_err());
    }

    #[test]
    fn strict_history_checks_total_bytes_and_rows_before_owned_response_allocation() {
        let xml = frame(&(bar("20261006", "") + &bar("20261007", "")), "true");
        assert_eq!(
            parse_bar_response_strict(&xml, 10, xml.len() - 1).unwrap_err(),
            StrictHistoryError::ByteLimit
        );
        assert_eq!(
            parse_bar_response_strict(&xml, 1, xml.len()).unwrap_err(),
            StrictHistoryError::RowLimit
        );
        assert_eq!(
            parse_bar_response_strict(&xml, 2, xml.len())
                .unwrap()
                .bars
                .len(),
            2
        );
        assert!(
            parse_bar_response_strict(&frame("", "true"), 0, 1000)
                .unwrap()
                .bars
                .is_empty()
        );
    }

    #[test]
    fn strict_history_requires_explicit_completion_and_refuses_duplicate_or_unknown_information() {
        for complete in ["", "TRUE", "1", "no"] {
            assert!(parse(&frame("", complete)).is_err());
        }
        let xml = frame("", "true");
        assert!(parse(&xml.replace("<eoq>true</eoq>", "")).is_err());
        assert!(parse(&xml.replace("<eoq>true</eoq>", "<eoq>true</eoq><eoq>false</eoq>")).is_err());
        assert!(
            parse(&xml.replace(
                "<Events>",
                "<warning>notice must not vanish</warning><Events>"
            ))
            .is_err()
        );
        assert!(parse(&frame("<Notice>warning</Notice>", "true")).is_err());
        assert!(
            parse(&frame(
                &bar("20261006", "<count>1</count><count>2</count>"),
                "true"
            ))
            .is_err()
        );
    }

    #[test]
    fn strict_history_accepts_simple_xml_declaration_and_session_markers_without_inventing_range() {
        let events = format!(
            "<Open><time>20261006-14:30:00</time><refDate>20261006</refDate></Open>{}<Close><time>20261006-21:00:00</time></Close>",
            bar("20261006", "")
        );
        let response = parse(
            &("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n".to_owned() + &frame(&events, "true")),
        )
        .unwrap();
        assert_eq!(response.bars.len(), 1);
        assert!(response.is_complete);
        assert!(parse(&frame("<Open><refDate>20261006</refDate></Open>", "true")).is_err());
    }

    #[test]
    fn strict_history_rejects_invalid_declarations_entities_control_text_and_excessive_depth() {
        let xml = frame(&bar("20261006", ""), "true");
        for prefix in ["<?xml junk?>", "<?xml version=\"1.0\" <broken>?>"] {
            assert!(parse(&(prefix.to_owned() + &xml)).is_err());
        }
        for time in ["date\0", "date&bogus;", "date\u{fffe}"] {
            assert!(parse(&frame(&bar(time, ""), "true")).is_err());
        }
        let nested = "<x>".repeat(17) + &"</x>".repeat(17);
        assert!(parse(&frame(&nested, "true")).is_err());
    }

    #[test]
    fn strict_history_preserves_upstream_bar_end_and_leg_average_as_distinct_raw_metadata() {
        let xml = frame(
            &bar(
                "20260227-20:30:00",
                "<endTime>20260227-20:31:00</endTime><timeAvg>266.466</timeAvg>",
            ),
            "true",
        );
        let response = parse(&xml).unwrap();
        assert_eq!(
            response.bars[0].end_time.as_deref(),
            Some("20260227-20:31:00")
        );
        assert_eq!(response.bars[0].time_average, Some(266.466));
        assert_eq!(response.bars[0].wap, None);
        assert_eq!(
            parse(&frame(&bar("20261006", ""), "false")).unwrap().bars[0].end_time,
            None
        );
        assert_eq!(
            parse(&frame(&bar("20261006", "<endTime/><timeAvg/>"), "true"))
                .unwrap()
                .bars[0]
                .time_average,
            None
        );
        for metadata in [
            "<timeAvg>NaN</timeAvg>",
            "<timeAvg>bad</timeAvg>",
            "<endTime>x</endTime><endTime>y</endTime>",
            "<timeAvg>1</timeAvg><timeAvg>2</timeAvg>",
        ] {
            assert!(parse(&frame(&bar("20261006", metadata), "true")).is_err());
        }
    }

    #[test]
    fn strict_history_preserves_legacy_parser_required_defaults_and_upstream_missing_stats() {
        let xml = frame("<Bar><time>20261006</time><open>bad</open></Bar>", "true");
        let legacy = super::super::parse_bar_response(&xml).unwrap();
        assert_eq!(legacy.bars[0].open, 0.0);
        // Reconciled upstream represents missing statistics with -1 (ibx#429),
        // while malformed/missing required OHLC still becomes an apparent zero.
        assert_eq!(legacy.bars[0].count, -1);
        assert_eq!(legacy.bars[0].volume, -1);
        assert_eq!(legacy.bars[0].wap, -1.0);
        assert!(legacy.start.is_empty());
        assert!(legacy.end.is_empty());
        assert!(parse(&xml).is_err());
    }

    #[test]
    fn strict_history_accepts_upstreams_captured_partial_five_second_bar() {
        // From historical::tests::five_second_bars_merge_as_captured.
        let xml = "<ResultSetBar><id>cf60</id><eoq>true</eoq><tz>US/Eastern</tz><Events>\
            <Open><time>20261002-08:00:00</time><refDate>20261002</refDate></Open>\
            <Bar><time>20261002-08:00:00</time><endTime>20261002-08:56:35</endTime>\
            <open>331.05</open><close>331.45</close><high>331.59</high><low>330.55</low>\
            <weightedAvg>331.285</weightedAvg><volume>49528</volume><count>522</count></Bar>\
            <Close><time>20261003-00:00:00</time></Close></Events></ResultSetBar>";
        let strict = parse(xml).unwrap();
        let legacy = super::super::parse_bar_response(xml).unwrap();
        assert_eq!(strict.query_id, legacy.query_id);
        assert_eq!(strict.timezone, legacy.timezone);
        assert_eq!(strict.is_complete, legacy.is_complete);
        let b = &strict.bars[0];
        let old = &legacy.bars[0];
        assert_eq!(
            (b.time.as_str(), b.open, b.high, b.low, b.close),
            (old.time.as_str(), old.open, old.high, old.low, old.close)
        );
        assert_eq!(b.volume, Some(49528));
        assert_eq!(b.count, Some(522));
        assert_eq!(b.wap, Some(331.285));
        assert_eq!(b.end_time.as_deref(), Some("20261002-08:56:35"));
        assert_eq!(b.time_average, None);
    }

    #[test]
    fn strict_history_accepts_upstreams_bid_ask_leg_sample() {
        let xml = "<ResultSetBar><id>q</id><eoq>true</eoq><Events>\
            <Bar><time>20260227-20:30:00</time><endTime>20260227-20:31:00</endTime>\
            <open>266.63</open><close>266.33</close><high>266.63</high><low>266.3</low>\
            <timeAvg>266.466</timeAvg></Bar></Events></ResultSetBar>";
        let response = parse(xml).unwrap();
        let b = &response.bars[0];
        assert_eq!(
            (b.open, b.high, b.low, b.close),
            (266.63, 266.63, 266.3, 266.33)
        );
        assert_eq!(b.time_average, Some(266.466));
        assert_eq!(b.wap, None);
        assert_eq!(b.volume, None);
        assert_eq!(b.count, None);
        assert!(response.timezone.is_empty());
    }

    #[test]
    fn strict_history_rejects_every_truncation_and_multiple_documents() {
        let xml = frame(&bar("20261006", ""), "true");
        for end in 0..xml.len() {
            assert!(parse(&xml[..end]).is_err(), "prefix at {end}");
        }
        assert!(parse(&(xml.clone() + &xml)).is_err());
        assert!(parse(&xml.replace("<id>hist_7</id>", "<id>hist_7</id><id>other</id>")).is_err());
        assert!(parse(&xml.replace("<Events>", "<Events extra='x'>")).is_err());
    }

    proptest::proptest! {
        #[test]
        fn strict_history_arbitrary_utf8_is_panic_free_and_respects_limits(
            chars in proptest::collection::vec(proptest::char::any(), 0..512),
            max_rows in 0usize..12,
            max_bytes in 0usize..2048,
        ) {
            let input: String = chars.into_iter().collect();
            let result = parse_bar_response_strict(&input, max_rows, max_bytes);
            if input.len() > max_bytes {
                proptest::prop_assert_eq!(result.unwrap_err(), StrictHistoryError::ByteLimit);
            } else if let Ok(response) = result {
                proptest::prop_assert!(response.bars.len() <= max_rows);
                proptest::prop_assert!(response.bars.iter().all(|b|
                    b.open.is_finite() && b.high.is_finite() && b.low.is_finite() && b.close.is_finite()));
            }
        }

        #[test]
        fn strict_history_generated_valid_rows_round_trip_and_enforce_admission(
            opens in proptest::collection::vec(-1_000_000i32..1_000_000, 0..12),
            max_rows in 0usize..12,
            complete in proptest::bool::ANY,
        ) {
            let events: String = opens.iter().enumerate().map(|(i, open)| format!(
                "<Bar><time>row_{i}</time><open>{open}</open><high>{open}</high>\
                <low>{open}</low><close>{open}</close></Bar>"
            )).collect();
            let xml = frame(&events, if complete { "true" } else { "false" });
            let result = parse_bar_response_strict(&xml, max_rows, xml.len());
            if opens.len() > max_rows {
                proptest::prop_assert_eq!(result.unwrap_err(), StrictHistoryError::RowLimit);
            } else {
                let response = result.unwrap();
                proptest::prop_assert_eq!(response.is_complete, complete);
                proptest::prop_assert_eq!(response.bars.len(), opens.len());
                for (i, b) in response.bars.iter().enumerate() {
                    proptest::prop_assert_eq!(&b.time, &format!("row_{i}"));
                    proptest::prop_assert_eq!(b.open, f64::from(opens[i]));
                    proptest::prop_assert_eq!(b.volume, None);
                }
            }
        }
    }
}
