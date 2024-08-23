use std::{any::Any, collections::HashMap, pin::Pin, sync::Arc};

use crate::{
    managers::sheet_manager::SheetManager,
    tools::{argument::ToolArgument, rust_tools::RustTool, shinkai_tool::ShinkaiTool},
};
use csv::ReaderBuilder;
use shinkai_message_primitives::schemas::sheet::{ColumnBehavior, ColumnDefinition};
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct SheetRustFunctions;

// Function to detect the delimiter
fn detect_delimiter(csv_data: &str) -> u8 {
    let mut comma_count = 0;
    let mut semicolon_count = 0;
    let mut tab_count = 0;

    for line in csv_data.lines().take(10) {
        comma_count += line.matches(',').count();
        semicolon_count += line.matches(';').count();
        tab_count += line.matches('\t').count();
    }

    if comma_count >= semicolon_count && comma_count >= tab_count {
        b','
    } else if semicolon_count >= comma_count && semicolon_count >= tab_count {
        b';'
    } else {
        b'\t'
    }
}

// Type alias for the unified function signature
type SheetToolFunction = fn(
    Arc<Mutex<SheetManager>>,
    String,
    HashMap<String, Box<dyn Any + Send>>,
) -> Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>>;

impl SheetRustFunctions {
    pub async fn create_new_column_with_values(
        sheet_manager: Arc<Mutex<SheetManager>>,
        sheet_id: String,
        args: HashMap<String, Box<dyn Any + Send>>,
    ) -> Result<String, String> {
        let values = args
            .get("values")
            .ok_or_else(|| "Missing argument: values".to_string())?
            .downcast_ref::<String>()
            .ok_or_else(|| "Invalid argument for values".to_string())?
            .split(',')
            .map(|s| s.trim().to_string())
            .collect::<Vec<String>>();

        // Create a new column of type Text
        let column_definition = ColumnDefinition {
            id: Uuid::new_v4().to_string(),
            name: "New Column".to_string(),
            behavior: ColumnBehavior::Text,
        };

        // Set the new column
        {
            let mut sheet_manager = sheet_manager.lock().await;
            sheet_manager.set_column(&sheet_id, column_definition.clone()).await?;
        }

        // Ensure the number of rows matches the number of values
        {
            let mut sheet_manager = sheet_manager.lock().await;
            while {
                let (sheet, _) = sheet_manager.sheets.get_mut(&sheet_id).ok_or("Sheet ID not found")?;
                sheet.rows.len() < values.len()
            } {
                sheet_manager.add_row(&sheet_id, None).await?;
            }
        }

        // Set values for the new column
        let row_ids: Vec<String> = {
            let sheet_manager = sheet_manager.lock().await;
            let (sheet, _) = sheet_manager.sheets.get(&sheet_id).ok_or("Sheet ID not found")?;
            sheet.display_rows.clone()
        };

        for (row_index, value) in values.iter().enumerate() {
            let row_id = row_ids.get(row_index).ok_or("Row ID not found")?.clone();
            let mut sheet_manager = sheet_manager.lock().await;
            sheet_manager
                .set_cell_value(&sheet_id, row_id, column_definition.id.clone(), value.clone())
                .await?;
        }

        Ok("Column created successfully".to_string())
    }

    pub async fn update_column_with_values(
        sheet_manager: Arc<Mutex<SheetManager>>,
        sheet_id: String,
        args: HashMap<String, Box<dyn Any + Send>>,
    ) -> Result<String, String> {
        let column_position = args
            .get("column_position")
            .ok_or_else(|| "Missing argument: column_position".to_string())?
            .downcast_ref::<String>()
            .ok_or_else(|| "Invalid argument for column position".to_string())?
            .parse::<usize>()
            .map_err(|_| "Invalid column position format".to_string())?;
        let values = args
            .get("values")
            .ok_or_else(|| "Missing argument: values".to_string())?
            .downcast_ref::<String>()
            .ok_or_else(|| "Invalid argument for values".to_string())?
            .split(',')
            .map(|s| s.trim().to_string())
            .collect::<Vec<String>>();

        // Adjust column_position to be 0-based
        let column_position = column_position
            .checked_sub(1)
            .ok_or_else(|| "Invalid column position".to_string())?;

        // Lock the sheet manager to access the sheet
        let (column_id, row_ids) = {
            let mut sheet_manager = sheet_manager.lock().await;
            let (sheet, _) = sheet_manager.sheets.get_mut(&sheet_id).ok_or("Sheet ID not found")?;

            // Ensure the column position is valid
            if column_position >= sheet.columns.len() {
                return Err("Invalid column position".to_string());
            }

            // Get the column ID
            let column_id = sheet
                .display_columns
                .get(column_position)
                .ok_or("Column ID not found")?
                .clone();

            (column_id, sheet.display_rows.clone())
        };

        // Ensure the number of rows matches the number of values
        {
            let mut sheet_manager = sheet_manager.lock().await;
            while {
                let (sheet, _) = sheet_manager.sheets.get_mut(&sheet_id).ok_or("Sheet ID not found")?;
                sheet.rows.len() < values.len()
            } {
                sheet_manager.add_row(&sheet_id, None).await?;
            }
        }

        // Clean existing values in the column
        for row_id in &row_ids {
            let mut sheet_manager = sheet_manager.lock().await;
            sheet_manager
                .set_cell_value(&sheet_id, row_id.clone(), column_id.clone(), "".to_string())
                .await?;
        }

        // Set new values for the column
        for (row_index, value) in values.iter().enumerate() {
            let row_id = row_ids.get(row_index).ok_or("Row ID not found")?.clone();
            let mut sheet_manager = sheet_manager.lock().await;
            sheet_manager
                .set_cell_value(&sheet_id, row_id, column_id.clone(), value.clone())
                .await?;
        }

        Ok("Column updated successfully".to_string())
    }

    pub async fn replace_value_at_position(
        sheet_manager: Arc<Mutex<SheetManager>>,
        sheet_id: String,
        args: HashMap<String, Box<dyn Any + Send>>,
    ) -> Result<String, String> {
        let column_position = args
            .get("column_position")
            .ok_or_else(|| "Missing argument: column_position".to_string())?
            .downcast_ref::<String>()
            .ok_or_else(|| "Invalid argument for column position".to_string())?
            .parse::<usize>()
            .map_err(|_| "Invalid column position format".to_string())?;
        let row_position = args
            .get("row_position")
            .ok_or_else(|| "Missing argument: row_position".to_string())?
            .downcast_ref::<String>()
            .ok_or_else(|| "Invalid argument for row position".to_string())?
            .parse::<usize>()
            .map_err(|_| "Invalid row position format".to_string())?;
        let new_value = args
            .get("new_value")
            .ok_or_else(|| "Missing argument: new_value".to_string())?
            .downcast_ref::<String>()
            .ok_or_else(|| "Invalid argument for new value".to_string())?
            .clone();

        // Adjust column_position and row_position to be 0-based
        let column_position = column_position
            .checked_sub(1)
            .ok_or_else(|| "Invalid column position".to_string())?;
        let row_position = row_position
            .checked_sub(1)
            .ok_or_else(|| "Invalid row position".to_string())?;

        // Lock the sheet manager to access the sheet
        let (column_id, row_id) = {
            let mut sheet_manager = sheet_manager.lock().await;
            let (sheet, _) = sheet_manager.sheets.get_mut(&sheet_id).ok_or("Sheet ID not found")?;

            // Ensure the column and row positions are valid
            if column_position >= sheet.columns.len() {
                return Err("Invalid column position".to_string());
            }
            if row_position >= sheet.rows.len() {
                return Err("Invalid row position".to_string());
            }

            // Get the column ID and row ID
            let column_id = sheet
                .display_columns
                .get(column_position)
                .ok_or("Column ID not found")?
                .clone();
            let row_id = sheet.display_rows.get(row_position).ok_or("Row ID not found")?.clone();

            (column_id, row_id)
        };

        // Set the new value for the specified cell
        let mut sheet_manager = sheet_manager.lock().await;
        sheet_manager
            .set_cell_value(&sheet_id, row_id, column_id, new_value)
            .await?;

        Ok("Value replaced successfully".to_string())
    }

    pub async fn create_new_columns_with_csv(
        sheet_manager: Arc<Mutex<SheetManager>>,
        sheet_id: String,
        args: HashMap<String, Box<dyn Any + Send>>,
    ) -> Result<String, String> {
        let csv_data = args
            .get("csv_data")
            .ok_or_else(|| "Missing argument: csv_data".to_string())?
            .downcast_ref::<String>()
            .ok_or_else(|| "Invalid argument for csv_data".to_string())?;

        let csv_data = csv_data.replace("\\n", "\n");

        let delimiter = detect_delimiter(&csv_data);
        eprintln!("Detected delimiter: {}", delimiter as char);

        let mut reader = ReaderBuilder::new()
            .delimiter(delimiter)
            .flexible(true)
            .from_reader(csv_data.as_bytes());
        let headers = reader.headers().map_err(|e| e.to_string())?.clone();
        eprintln!("Headers: {:?}", headers);

        let records: Vec<csv::StringRecord> = reader.records().collect::<Result<_, _>>().map_err(|e| {
            eprintln!("Error reading records: {}", e);
            e.to_string()
        })?;

        let records: Vec<csv::StringRecord> = records
            .into_iter()
            .map(|mut record| {
                let record: &mut csv::StringRecord = &mut record;
                while record.len() < headers.len() {
                    record.push_field("");
                }
                record.clone()
            })
            .collect();
        eprintln!("Records: {:?}", records);

        // Create new columns based on headers
        let column_definitions: Vec<ColumnDefinition> = headers
            .iter()
            .map(|header| ColumnDefinition {
                id: Uuid::new_v4().to_string(),
                name: header.to_string(),
                behavior: ColumnBehavior::Text,
            })
            .collect();
        eprintln!("Column Definitions: {:?}", column_definitions);

        // Set the new columns
        {
            let mut sheet_manager = sheet_manager.lock().await;
            for column_definition in &column_definitions {
                sheet_manager.set_column(&sheet_id, column_definition.clone()).await?;
            }
        }

        // Ensure the number of rows matches the number of records
        {
            let mut sheet_manager = sheet_manager.lock().await;
            while {
                let (sheet, _) = sheet_manager.sheets.get_mut(&sheet_id).ok_or("Sheet ID not found")?;
                sheet.rows.len() < records.len()
            } {
                sheet_manager.add_row(&sheet_id, None).await?;
            }
        }

        // Set values for the new columns
        let row_ids: Vec<String> = {
            let sheet_manager = sheet_manager.lock().await;
            let (sheet, _) = sheet_manager.sheets.get(&sheet_id).ok_or("Sheet ID not found")?;
            sheet.display_rows.clone()
        };

        for (row_index, record) in records.iter().enumerate() {
            let row_id = row_ids.get(row_index).ok_or("Row ID not found")?.clone();
            for (col_index, value) in record.iter().enumerate() {
                let column_definition = &column_definitions[col_index];
                let mut sheet_manager = sheet_manager.lock().await;
                sheet_manager
                    .set_cell_value(
                        &sheet_id,
                        row_id.clone(),
                        column_definition.id.clone(),
                        value.to_string(),
                    )
                    .await?;
            }
        }

        Ok("Columns created successfully".to_string())
    }

    fn get_tool_map() -> HashMap<&'static str, SheetToolFunction> {
        let mut tool_map: HashMap<&str, SheetToolFunction> = HashMap::new();
        tool_map.insert("create_new_column_with_values", |sheet_manager, sheet_id, args| {
            Box::pin(SheetRustFunctions::create_new_column_with_values(
                sheet_manager,
                sheet_id,
                args,
            ))
        });
        tool_map.insert("update_column_with_values", |sheet_manager, sheet_id, args| {
            Box::pin(SheetRustFunctions::update_column_with_values(
                sheet_manager,
                sheet_id,
                args,
            ))
        });
        tool_map.insert("replace_value_at_position", |sheet_manager, sheet_id, args| {
            Box::pin(SheetRustFunctions::replace_value_at_position(
                sheet_manager,
                sheet_id,
                args,
            ))
        });
        tool_map.insert("create_new_columns_with_csv", |sheet_manager, sheet_id, args| {
            Box::pin(SheetRustFunctions::create_new_columns_with_csv(
                sheet_manager,
                sheet_id,
                args,
            ))
        });
        tool_map
    }

    pub fn get_tool_function(name: String) -> Option<SheetToolFunction> {
        let tool_map = Self::get_tool_map();
        tool_map.get(name.as_str()).copied()
    }

    pub fn sheet_rust_fn() -> Vec<ShinkaiTool> {
        // Add the tool definition for create_new_column_with_values
        let create_new_column_tool = RustTool::new(
            "create_new_column_with_values".to_string(),
            "Creates a new column with the provided values. Values should be separated by commas. Example: 'value1, value2, value3'".to_string(),
            vec![
                ToolArgument::new(
                    "values".to_string(),
                    "string".to_string(),
                    "The values to populate the new column, separated by commas".to_string(),
                    true,
                ),
            ],
            None,
        );

        // Add the tool definition for update_column_with_values
        let update_column_tool = RustTool::new(
            "update_column_with_values".to_string(),
            "Updates an existing column with the provided values. Values should be separated by commas. Example: 'value1, value2, value3'".to_string(),
            vec![
                ToolArgument::new(
                    "column_position".to_string(),
                    "usize".to_string(),
                    "The position of the column to update".to_string(),
                    true,
                ),
                ToolArgument::new(
                    "values".to_string(),
                    "string".to_string(),
                    "The values to update the column with, separated by commas".to_string(),
                    true,
                ),
            ],
            None,
        );

        // Add the tool definition for replace_value_at_position
        let replace_value_tool = RustTool::new(
            "replace_value_at_position".to_string(),
            "Replaces the value at the specified column and row position. Example: 'column_position, row_position, new_value'".to_string(),
            vec![
                ToolArgument::new(
                    "column_position".to_string(),
                    "usize".to_string(),
                    "The position of the column".to_string(),
                    true,
                ),
                ToolArgument::new(
                    "row_position".to_string(),
                    "usize".to_string(),
                    "The position of the row".to_string(),
                    true,
                ),
                ToolArgument::new(
                    "new_value".to_string(),
                    "string".to_string(),
                    "The new value to set".to_string(),
                    true,
                ),
            ],
            None,
        );

        // Add the tool definition for create_new_columns_with_csv
        let create_new_columns_tool = RustTool::new(
            "create_new_columns_with_csv".to_string(),
            "Creates new columns with the provided CSV data. Example: 'column1;column2\nvalue1;value2' It also supports comma separators.".to_string(),
            vec![ToolArgument::new(
                "csv_data".to_string(),
                "string".to_string(),
                "The CSV data to populate the new columns".to_string(),
                true,
            )],
            None,
        );

        vec![
            ShinkaiTool::Rust(create_new_column_tool, true),
            ShinkaiTool::Rust(update_column_tool, true),
            ShinkaiTool::Rust(replace_value_tool, true),
            ShinkaiTool::Rust(create_new_columns_tool, true),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::ShinkaiDB, llm_provider::job_manager::JobManagerTrait, network::ws_manager::WSUpdateHandler};
    use async_trait::async_trait;
    use futures::Future;
    use shinkai_message_primitives::{
        schemas::shinkai_name::ShinkaiName,
        shinkai_message::shinkai_message_schemas::{JobCreationInfo, JobMessage},
    };
    use shinkai_vector_resources::utils::hash_string;
    use std::{fs, path::Path, sync::Arc};
    use tokio::sync::Mutex;

    struct MockJobManager;

    #[async_trait]
    impl JobManagerTrait for MockJobManager {
        fn create_job<'a>(
            &'a mut self,
            _job_creation_info: JobCreationInfo,
            _user_profile: &'a ShinkaiName,
            _agent_id: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
            Box::pin(async move { Ok("mock_job_id".to_string()) })
        }

        fn queue_job_message<'a>(
            &'a mut self,
            _job_message: &'a JobMessage,
            _user_profile: &'a ShinkaiName,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
            Box::pin(async move { Ok("mock_job_id".to_string()) })
        }
    }

    pub fn setup() {
        let path = Path::new("db_tests/");
        let _ = fs::remove_dir_all(path);

        let lance_path = Path::new("lance_db_tests/");
        let _ = fs::remove_dir_all(lance_path);
    }

    pub fn create_testing_db(node_name: String) -> ShinkaiDB {
        let db_path = format!("db_tests/{}", hash_string(&node_name));
        ShinkaiDB::new(&db_path).unwrap()
    }

    #[tokio::test]
    async fn test_set_column_with_mock_job_manager() {
        setup();
        let node_name = "@@test.arb-sep-shinkai".to_string();
        let db = create_testing_db(node_name.clone());
        let db = Arc::new(db);
        let node_name = ShinkaiName::new(node_name).unwrap();
        let ws_manager: Option<Arc<Mutex<dyn WSUpdateHandler + Send>>> = None;

        let sheet_manager = Arc::new(Mutex::new(
            SheetManager::new(Arc::downgrade(&db), node_name, ws_manager)
                .await
                .unwrap(),
        ));

        let mock_job_manager = Arc::new(Mutex::new(MockJobManager));
        sheet_manager.lock().await.set_job_manager(mock_job_manager);

        let sheet_id = sheet_manager.lock().await.create_empty_sheet().unwrap();

        // Call create_new_column_with_values with the values: "USA, Chile, Canada"
        let mut args = HashMap::new();
        args.insert(
            "values".to_string(),
            Box::new("USA, Chile, Canada".to_string()) as Box<dyn Any + Send>,
        );
        let result =
            SheetRustFunctions::create_new_column_with_values(sheet_manager.clone(), sheet_id.clone(), args).await;
        assert!(result.is_ok(), "Creating new column with values should succeed");

        {
            let sheet_manager = sheet_manager.lock().await;
            let sheet = sheet_manager.get_sheet(&sheet_id).unwrap();
            assert_eq!(sheet.columns.len(), 1, "There should be one column in the sheet");
            assert_eq!(sheet.rows.len(), 3, "There should be three rows in the sheet");

            // Check the order of the first column
            let col_id = sheet.display_columns.get(0).expect("Column ID not found").clone();
            let expected_values = vec!["USA", "Chile", "Canada"];
            for (i, expected_value) in expected_values.iter().enumerate() {
                let row_id = sheet.display_rows.get(i).expect("Row ID not found").clone();
                let cell_value = sheet
                    .get_cell_value(row_id.clone(), col_id.clone())
                    .expect("Cell value not found");
                assert_eq!(
                    cell_value, *expected_value,
                    "The value in row {} of the first column should be '{}'",
                    i, expected_value
                );
            }
        }

        // Call create_new_column_with_values again with the value: "Italy"
        let mut args = HashMap::new();
        args.insert(
            "values".to_string(),
            Box::new("Italy".to_string()) as Box<dyn Any + Send>,
        );
        let result =
            SheetRustFunctions::create_new_column_with_values(sheet_manager.clone(), sheet_id.clone(), args).await;
        assert!(result.is_ok(), "Creating new column with a single value should succeed");

        {
            let sheet_manager = sheet_manager.lock().await;
            let sheet = sheet_manager.get_sheet(&sheet_id).unwrap();
            assert_eq!(sheet.columns.len(), 2, "There should be two columns in the sheet");
            assert_eq!(sheet.rows.len(), 3, "There should still be three rows in the sheet");

            // Check that "Italy" is in the first row, second column
            let row_id = sheet.display_rows.get(0).expect("Row ID not found").clone();
            let col_id = sheet.display_columns.get(1).expect("Column ID not found").clone();

            let cell_value = sheet
                .get_cell_value(row_id.clone(), col_id.clone())
                .expect("Cell value not found");
            assert_eq!(
                cell_value, "Italy",
                "The value in the first row, second column should be 'Italy'"
            );
        }
    }

    #[tokio::test]
    async fn test_update_column_with_values() {
        setup();
        let node_name = "@@test.arb-sep-shinkai".to_string();
        let db = create_testing_db(node_name.clone());
        let db = Arc::new(db);
        let node_name = ShinkaiName::new(node_name).unwrap();
        let ws_manager: Option<Arc<Mutex<dyn WSUpdateHandler + Send>>> = None;

        let sheet_manager = Arc::new(Mutex::new(
            SheetManager::new(Arc::downgrade(&db), node_name, ws_manager)
                .await
                .unwrap(),
        ));

        let mock_job_manager = Arc::new(Mutex::new(MockJobManager));
        sheet_manager.lock().await.set_job_manager(mock_job_manager);

        let sheet_id = sheet_manager.lock().await.create_empty_sheet().unwrap();

        // Create a new column with values: "USA, Chile, Canada"
        let mut args = HashMap::new();
        args.insert(
            "values".to_string(),
            Box::new("USA, Chile, Canada".to_string()) as Box<dyn Any + Send>,
        );
        let result =
            SheetRustFunctions::create_new_column_with_values(sheet_manager.clone(), sheet_id.clone(), args).await;
        assert!(result.is_ok(), "Creating new column with values should succeed");

        // Update the column with new values: "Italy, France"
        let mut args = HashMap::new();
        args.insert(
            "column_position".to_string(),
            Box::new("1".to_string()) as Box<dyn Any + Send>,
        );
        args.insert(
            "values".to_string(),
            Box::new("Italy, France".to_string()) as Box<dyn Any + Send>,
        );
        let result = SheetRustFunctions::update_column_with_values(sheet_manager.clone(), sheet_id.clone(), args).await;
        assert!(result.is_ok(), "Updating column with values should succeed");

        {
            let sheet_manager = sheet_manager.lock().await;
            let sheet = sheet_manager.get_sheet(&sheet_id).unwrap();
            assert_eq!(sheet.columns.len(), 1, "There should be one column in the sheet");
            assert_eq!(sheet.rows.len(), 3, "There should still be three rows in the sheet");

            // Check the updated values in the first column
            let col_id = sheet.display_columns.get(0).expect("Column ID not found").clone();
            let expected_values = vec!["Italy", "France", ""];
            for (i, expected_value) in expected_values.iter().enumerate() {
                let row_id = sheet.display_rows.get(i).expect("Row ID not found").clone();
                let cell_value = sheet
                    .get_cell_value(row_id.clone(), col_id.clone())
                    .expect("Cell value not found");
                assert_eq!(
                    cell_value, *expected_value,
                    "The value in row {} of the first column should be '{}'",
                    i, expected_value
                );
            }
        }
    }

    #[tokio::test]
    async fn test_replace_value_at_position() {
        setup();
        let node_name = "@@test.arb-sep-shinkai".to_string();
        let db = create_testing_db(node_name.clone());
        let db = Arc::new(db);
        let node_name = ShinkaiName::new(node_name).unwrap();
        let ws_manager: Option<Arc<Mutex<dyn WSUpdateHandler + Send>>> = None;

        let sheet_manager = Arc::new(Mutex::new(
            SheetManager::new(Arc::downgrade(&db), node_name, ws_manager)
                .await
                .unwrap(),
        ));

        let mock_job_manager = Arc::new(Mutex::new(MockJobManager));
        sheet_manager.lock().await.set_job_manager(mock_job_manager);

        let sheet_id = sheet_manager.lock().await.create_empty_sheet().unwrap();

        // Create a new column with values: "USA, Chile, Canada"
        let mut args = HashMap::new();
        args.insert(
            "values".to_string(),
            Box::new("USA, Chile, Canada".to_string()) as Box<dyn Any + Send>,
        );
        let result =
            SheetRustFunctions::create_new_column_with_values(sheet_manager.clone(), sheet_id.clone(), args).await;
        assert!(result.is_ok(), "Creating new column with values should succeed");

        // Replace the value at position (0, 1) with "Brazil"
        let mut args = HashMap::new();
        args.insert(
            "column_position".to_string(),
            Box::new("1".to_string()) as Box<dyn Any + Send>,
        );
        args.insert(
            "row_position".to_string(),
            Box::new("2".to_string()) as Box<dyn Any + Send>,
        );
        args.insert(
            "new_value".to_string(),
            Box::new("Brazil".to_string()) as Box<dyn Any + Send>,
        );
        let result = SheetRustFunctions::replace_value_at_position(sheet_manager.clone(), sheet_id.clone(), args).await;
        assert!(result.is_ok(), "Replacing value at position should succeed");

        {
            let sheet_manager = sheet_manager.lock().await;
            let sheet = sheet_manager.get_sheet(&sheet_id).unwrap();
            assert_eq!(sheet.columns.len(), 1, "There should be one column in the sheet");
            assert_eq!(sheet.rows.len(), 3, "There should be three rows in the sheet");

            // Check the updated value in the first column, second row
            let col_id = sheet.display_columns.get(0).expect("Column ID not found").clone();
            let row_id = sheet.display_rows.get(1).expect("Row ID not found").clone();
            let cell_value = sheet
                .get_cell_value(row_id.clone(), col_id.clone())
                .expect("Cell value not found");
            assert_eq!(
                cell_value, "Brazil",
                "The value in the first column, second row should be 'Brazil'"
            );
        }
    }

    #[tokio::test]
    async fn test_create_new_columns_with_csv() {
        setup();
        let node_name = "@@test.arb-sep-shinkai".to_string();
        let db = create_testing_db(node_name.clone());
        let db = Arc::new(db);
        let node_name = ShinkaiName::new(node_name).unwrap();
        let ws_manager: Option<Arc<Mutex<dyn WSUpdateHandler + Send>>> = None;

        let sheet_manager = Arc::new(Mutex::new(
            SheetManager::new(Arc::downgrade(&db), node_name, ws_manager)
                .await
                .unwrap(),
        ));

        let mock_job_manager = Arc::new(Mutex::new(MockJobManager));
        sheet_manager.lock().await.set_job_manager(mock_job_manager);

        let sheet_id = sheet_manager.lock().await.create_empty_sheet().unwrap();

        // Create new columns with CSV data
        let csv_data = "Name,Age,Location\nAlice,30,USA\nBob,25,UK\nCharlie,35,Canada";
        let mut args = HashMap::new();
        args.insert(
            "csv_data".to_string(),
            Box::new(csv_data.to_string()) as Box<dyn Any + Send>,
        );
        let result =
            SheetRustFunctions::create_new_columns_with_csv(sheet_manager.clone(), sheet_id.clone(), args).await;
        assert!(result.is_ok(), "Creating new columns with CSV data should succeed");

        {
            let sheet_manager = sheet_manager.lock().await;
            let sheet = sheet_manager.get_sheet(&sheet_id).unwrap();
            assert_eq!(sheet.columns.len(), 3, "There should be three columns in the sheet");
            assert_eq!(sheet.rows.len(), 3, "There should be three rows in the sheet");

            // Check the values in the columns
            let expected_values = vec![
                vec!["Alice", "30", "USA"],
                vec!["Bob", "25", "UK"],
                vec!["Charlie", "35", "Canada"],
            ];
            for (row_index, expected_row) in expected_values.iter().enumerate() {
                let row_id = sheet.display_rows.get(row_index).expect("Row ID not found").clone();
                for (col_index, expected_value) in expected_row.iter().enumerate() {
                    let col_id = sheet
                        .display_columns
                        .get(col_index)
                        .expect("Column ID not found")
                        .clone();
                    let cell_value = sheet
                        .get_cell_value(row_id.clone(), col_id.clone())
                        .expect("Cell value not found");
                    assert_eq!(
                        cell_value, *expected_value,
                        "The value in row {}, column {} should be '{}'",
                        row_index, col_index, expected_value
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn test_create_new_columns_with_large_csv() {
        setup();
        let node_name = "@@test.arb-sep-shinkai".to_string();
        let db = create_testing_db(node_name.clone());
        let db = Arc::new(db);
        let node_name = ShinkaiName::new(node_name).unwrap();
        let ws_manager: Option<Arc<Mutex<dyn WSUpdateHandler + Send>>> = None;

        let sheet_manager = Arc::new(Mutex::new(
            SheetManager::new(Arc::downgrade(&db), node_name, ws_manager)
                .await
                .unwrap(),
        ));

        let mock_job_manager = Arc::new(Mutex::new(MockJobManager));
        sheet_manager.lock().await.set_job_manager(mock_job_manager);

        let sheet_id = sheet_manager.lock().await.create_empty_sheet().unwrap();

        // Create new columns with large CSV data from JSON content
        let json_content = r#"{
            "data": {
                "columnsCount": 16,
                "rowsCount": 10,
                "tableCsv": "Rank;Compare;Name;1d Change;7d Change;1m Change;TVL;Fees 7d;Revenue 7d;Mcap/TVL;Fees 24h;Fees 30d;Revenue 24h;Borrowed;Supplied;Supplied/TVL\n1;;AAVE12 chains;-1.93%;-3.07%;-15.84%;$11.473b;$4.49m;$461,609;0.17;$1.38m;$21.9m;$18,229;;;\n2;;JustLend1 chain;+2.24%;+2.93%;-8.44%;$5.861b;$71,060;$6,428;0.05;$11,496;$281,726;$1,038;94.25m;5.955b;1.02\n3;;Spark2 chains;-2.01%;-6.01%;-21.04%;$2.538b;;;;;;;638.24m;3.176b;1.25\n4;;Compound Finance6 chains;-1.45%;-2.47%;-23.01%;$1.891b;$142,443;$24,816;0.21;$20,752;$1.14m;$3,616;;;\n5;;Venus4 chains;-0.61%;+0.68%;-10.72%;$1.593b;$747,095;$108,184;0.07;$111,968;$2.9m;$17,483;;;\n6;;Morpho2 chains;-3.57%;-9.11%;-23.94%;$1.458b;$504,076;;;$68,897;$2.08m;;;;\n7;;Kamino Lend1 chain;+2.36%;+4.86%;+5.32%;$1.275b;$1.15m;$224,387;;$161,367;$4.77m;$31,234;;;\n8;;LayerBank8 chains;-2.14%;-1.94%;-22.10%;$674.22m;;;;;;;16.22m;690.45m;1.02\n9;;Fluid3 chains;-2.23%;+8.23%;-8.37%;$365.3m;$210,172;$21,027;;$30,200;$1.28m;$3,035;292.24m;657.53m;1.8\n10;;marginfi Lending1 chain;-0.99%;+0.31%;-20.11%;$339.8m;;;;;;;;"
            }
        }"#;

        let parsed_json: serde_json::Value = serde_json::from_str(json_content).unwrap();
        let csv_data = parsed_json["data"]["tableCsv"].as_str().unwrap();

        let mut args = HashMap::new();
        args.insert(
            "csv_data".to_string(),
            Box::new(csv_data.to_string()) as Box<dyn Any + Send>,
        );
        let result =
            SheetRustFunctions::create_new_columns_with_csv(sheet_manager.clone(), sheet_id.clone(), args).await;
        assert!(
            result.is_ok(),
            "Creating new columns with large CSV data should succeed"
        );

        {
            let sheet_manager = sheet_manager.lock().await;
            let sheet = sheet_manager.get_sheet(&sheet_id).unwrap();
            assert_eq!(sheet.columns.len(), 16, "There should be sixteen columns in the sheet");
            assert_eq!(sheet.rows.len(), 10, "There should be ten rows in the sheet");

            // Check the values in the columns
            let expected_values = vec![
                vec![
                    "1",
                    "",
                    "AAVE12 chains",
                    "-1.93%",
                    "-3.07%",
                    "-15.84%",
                    "$11.473b",
                    "$4.49m",
                    "$461,609",
                    "0.17",
                    "$1.38m",
                    "$21.9m",
                    "$18,229",
                    "",
                    "",
                    "",
                ],
                vec![
                    "2",
                    "",
                    "JustLend1 chain",
                    "+2.24%",
                    "+2.93%",
                    "-8.44%",
                    "$5.861b",
                    "$71,060",
                    "$6,428",
                    "0.05",
                    "$11,496",
                    "$281,726",
                    "$1,038",
                    "94.25m",
                    "5.955b",
                    "1.02",
                ],
                vec![
                    "3",
                    "",
                    "Spark2 chains",
                    "-2.01%",
                    "-6.01%",
                    "-21.04%",
                    "$2.538b",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "638.24m",
                    "3.176b",
                    "1.25",
                ],
                vec![
                    "4",
                    "",
                    "Compound Finance6 chains",
                    "-1.45%",
                    "-2.47%",
                    "-23.01%",
                    "$1.891b",
                    "$142,443",
                    "$24,816",
                    "0.21",
                    "$20,752",
                    "$1.14m",
                    "$3,616",
                    "",
                    "",
                    "",
                ],
                vec![
                    "5",
                    "",
                    "Venus4 chains",
                    "-0.61%",
                    "+0.68%",
                    "-10.72%",
                    "$1.593b",
                    "$747,095",
                    "$108,184",
                    "0.07",
                    "$111,968",
                    "$2.9m",
                    "$17,483",
                    "",
                    "",
                    "",
                ],
                vec![
                    "6",
                    "",
                    "Morpho2 chains",
                    "-3.57%",
                    "-9.11%",
                    "-23.94%",
                    "$1.458b",
                    "$504,076",
                    "",
                    "",
                    "$68,897",
                    "$2.08m",
                    "",
                    "",
                    "",
                    "",
                ],
                vec![
                    "7",
                    "",
                    "Kamino Lend1 chain",
                    "+2.36%",
                    "+4.86%",
                    "+5.32%",
                    "$1.275b",
                    "$1.15m",
                    "$224,387",
                    "",
                    "$161,367",
                    "$4.77m",
                    "$31,234",
                    "",
                    "",
                    "",
                ],
                vec![
                    "8",
                    "",
                    "LayerBank8 chains",
                    "-2.14%",
                    "-1.94%",
                    "-22.10%",
                    "$674.22m",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "16.22m",
                    "690.45m",
                    "1.02",
                ],
                vec![
                    "9",
                    "",
                    "Fluid3 chains",
                    "-2.23%",
                    "+8.23%",
                    "-8.37%",
                    "$365.3m",
                    "$210,172",
                    "$21,027",
                    "",
                    "$30,200",
                    "$1.28m",
                    "$3,035",
                    "292.24m",
                    "657.53m",
                    "1.8",
                ],
                vec![
                    "10",
                    "",
                    "marginfi Lending1 chain",
                    "-0.99%",
                    "+0.31%",
                    "-20.11%",
                    "$339.8m",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                    "",
                ],
            ];
            for (row_index, expected_row) in expected_values.iter().enumerate() {
                let row_id = sheet.display_rows.get(row_index).expect("Row ID not found").clone();
                for (col_index, expected_value) in expected_row.iter().enumerate() {
                    let col_id = sheet
                        .display_columns
                        .get(col_index)
                        .expect("Column ID not found")
                        .clone();
                    let cell_value = sheet
                        .get_cell_value(row_id.clone(), col_id.clone())
                        .expect("Cell value not found");
                    assert_eq!(
                        cell_value, *expected_value,
                        "The value in row {}, column {} should be '{}'",
                        row_index, col_index, expected_value
                    );
                }
            }
        }
    }
}
