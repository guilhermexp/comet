## ADDED Requirements

### Requirement: RFC 4180 CSV and TSV Parsing

The file preview loader SHALL parse CSV and TSV files conforming to RFC 4180 rules, respecting custom delimiters (`,` for CSV, `\t` for TSV), preserving delimiters inside quoted fields, decoding escaped quotes, and allowing multiline records.

#### Scenario: Commas inside quotes are not split

Test: `tests::parses_csv_respecting_quotes_commas_multiline_and_escapes`

- **WHEN** a CSV contains `"Silva, João",25`
- **THEN** it parses into 2 fields: `Silva, João` and `25`
- **AND** outer quotes are stripped from the displayed cell content

#### Scenario: Escaped quotes follow RFC 4180 without treating backslash as escape

Test: `tests::parses_csv_with_trailing_backslash_in_quoted_field`

- **WHEN** a CSV field contains escaped quotes `""`
- **THEN** it decodes to a single literal quote `"` in the cell value
- **AND** when a quoted field contains a backslash (e.g. `"C:\tmp\",42`), the backslash is preserved literally and does not escape the closing quote, parsing into 2 fields
#### Scenario: Multiline quoted fields form single record

Test: `tests::parses_csv_respecting_quotes_commas_multiline_and_escapes`

- **WHEN** a quoted CSV field spans across line breaks
- **THEN** it forms a single record rather than multiple records
- **AND** internal newlines are preserved within the field content

#### Scenario: Bounded rows and columns

Test: `tests::parses_tsv_and_clamps_limits`

- **WHEN** a large CSV or TSV file is parsed
- **THEN** rows are bounded to at most 2,000 and columns to at most 100
- **AND** excessive rows or columns are clamped safely
