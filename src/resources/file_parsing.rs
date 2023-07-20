use crate::resources::resource_errors::*;
use csv::Reader;
use pdf_extract;
use std::io::Cursor;

pub struct FileParser {}

impl FileParser {
    /// Parse CSV data from a buffer and attempt to automatically detect
    /// headers.
    ///
    /// # Arguments
    ///
    /// * `buffer` - A byte slice containing the CSV data.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<String>`. Each `String` represents a row in
    /// the CSV, and contains the column values for that row, concatenated
    /// together with commas. If an error occurs while parsing the CSV data,
    /// the `Result` will contain an `Error`.
    pub fn parse_csv_auto(buffer: &[u8]) -> Result<Vec<String>, ResourceError> {
        let mut reader = Reader::from_reader(Cursor::new(buffer));
        let headers = reader
            .headers()
            .map_err(|_| ResourceError::FailedCSVParsing)?
            .iter()
            .map(String::from)
            .collect::<Vec<String>>();

        let likely_header = headers.iter().all(|s| {
            let is_alphabetic = s.chars().all(|c| c.is_alphabetic() || c.is_whitespace());
            let no_duplicates = headers.iter().filter(|&item| item == s).count() == 1;
            let no_prohibited_chars = !s.contains(&['@', '#', '$', '%', '^', '&', '*'][..]);

            is_alphabetic && no_duplicates && no_prohibited_chars
        });

        Self::parse_csv(&buffer, likely_header)
    }

    /// Parse CSV data from a buffer.
    ///
    /// # Arguments
    ///
    /// * `buffer` - A byte slice containing the CSV data.
    /// * `header` - A boolean indicating whether to prepend column headers to
    ///   values.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<String>`. Each `String` represents a row in
    /// the CSV, and contains the column values for that row, concatenated
    /// together with commas. If an error occurs while parsing the CSV data,
    /// the `Result` will contain an `Error`.
    pub fn parse_csv(buffer: &[u8], header: bool) -> Result<Vec<String>, ResourceError> {
        let mut reader = Reader::from_reader(Cursor::new(buffer));
        let headers = if header {
            reader
                .headers()
                .map_err(|_| ResourceError::FailedCSVParsing)?
                .iter()
                .map(String::from)
                .collect::<Vec<String>>()
        } else {
            Vec::new()
        };

        let mut result = Vec::new();
        for record in reader.records() {
            let record = record.map_err(|_| ResourceError::FailedCSVParsing)?;
            let row: Vec<String> = if header {
                record
                    .iter()
                    .enumerate()
                    .map(|(i, e)| format!("{}: {}", headers[i], e))
                    .collect()
            } else {
                record.iter().map(String::from).collect()
            };
            let row_string = row.join(", ");
            result.push(row_string);
        }

        Ok(result)
    }

    /// Parse text from a PDF from a buffer.
    ///
    /// # Arguments
    ///
    /// * `buffer` - A byte slice containing the PDF data.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `String` of the extracted text from the PDF. If
    /// an error occurs while parsing the PDF data, the `Result` will
    /// contain an `Error`.
    pub fn parse_pdf(buffer: &[u8]) -> Result<String, ResourceError> {
        let text = pdf_extract::extract_text_from_mem(buffer).map_err(|_| ResourceError::FailedPDFParsing)?;

        Ok(text)
    }

    /// Parse CSV data from a file.
    ///
    /// # Arguments
    ///
    /// * `file_path` - A string slice representing the file path of the CSV
    ///   file.
    /// * `header` - A boolean indicating whether to prepend column headers to
    ///   values.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<Vec<String>>`. Each inner `Vec<String>`
    /// represents a row in the CSV, and contains the column values for that
    /// row. If an error occurs while parsing the CSV data, the `Result`
    /// will contain an `Error`.
    pub fn parse_csv_from_path(file_path: &str, header: bool) -> Result<Vec<String>, ResourceError> {
        let buffer = std::fs::read(file_path).map_err(|_| ResourceError::FailedCSVParsing)?;
        Self::parse_csv(&buffer, header)
    }

    /// Parse text from a PDF from a file.
    ///
    /// # Arguments
    ///
    /// * `file_path` - A string slice representing the file path of the PDF
    ///   file.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `String` of the extracted text from the PDF. If
    /// an error occurs while parsing the PDF data, the `Result` will
    /// contain an `Error`.
    pub fn parse_pdf_from_path(file_path: &str) -> Result<String, ResourceError> {
        let buffer = std::fs::read(file_path).map_err(|_| ResourceError::FailedPDFParsing)?;
        Self::parse_pdf(&buffer)
    }
}
