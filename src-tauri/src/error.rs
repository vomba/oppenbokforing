use serde::Serialize;
use specta::Type;

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldError {
    pub field: Option<String>,
    pub message: String,
    pub code: String,
}

#[derive(Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub details: Option<Vec<FieldError>>,
    #[serde(skip)]
    #[specta(skip)]
    pub(crate) unique_violation: bool,
}

impl AppError {
    pub fn validation(message: impl Into<String>, field: impl Into<String>) -> Self {
        Self {
            code: "validation_error".to_string(),
            message: message.into(),
            details: Some(vec![FieldError {
                field: Some(field.into()),
                message: "Invalid value".to_string(),
                code: "invalid_value".to_string(),
            }]),
            unique_violation: false,
        }
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self {
            code: "storage_error".to_string(),
            message: message.into(),
            details: None,
            unique_violation: false,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal_error".to_string(),
            message: message.into(),
            details: None,
            unique_violation: false,
        }
    }

    pub fn locked_period(message: impl Into<String>) -> Self {
        Self {
            code: "locked_period".to_string(),
            message: message.into(),
            details: None,
            unique_violation: false,
        }
    }

    pub fn workspace_not_open(message: impl Into<String>) -> Self {
        Self {
            code: "workspace_not_open".to_string(),
            message: message.into(),
            details: None,
            unique_violation: false,
        }
    }

    pub fn is_unique_violation(&self) -> bool {
        self.unique_violation
    }
}

pub fn is_sqlite_unique_violation(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(db) => {
            db.code().as_deref() == Some("2067")
                || db.message().contains("UNIQUE constraint failed")
        }
        _ => false,
    }
}

const STORAGE_ERROR_PUBLIC: &str =
    "A database operation failed. Try again or reopen the workspace.";

const IO_ERROR_PUBLIC: &str = "A file operation failed. Try again or reopen the workspace.";

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        #[cfg(debug_assertions)]
        eprintln!("io error (redacted from client): kind={:?}", error.kind());
        Self {
            code: "storage_error".to_string(),
            message: IO_ERROR_PUBLIC.to_string(),
            details: None,
            unique_violation: false,
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        let unique_violation = is_sqlite_unique_violation(&error);
        #[cfg(debug_assertions)]
        eprintln!("sqlx error (redacted from client): {error}");
        Self {
            code: "storage_error".to_string(),
            message: STORAGE_ERROR_PUBLIC.to_string(),
            details: None,
            unique_violation,
        }
    }
}

impl From<sqlx::migrate::MigrateError> for AppError {
    fn from(_error: sqlx::migrate::MigrateError) -> Self {
        Self {
            code: "storage_error".to_string(),
            message: STORAGE_ERROR_PUBLIC.to_string(),
            details: None,
            unique_violation: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppError, STORAGE_ERROR_PUBLIC};

    #[test]
    fn validation_error_includes_field_detail() {
        let error = AppError::validation("Workspace name is required", "name");

        assert_eq!(error.code, "validation_error");
        assert_eq!(error.details.unwrap()[0].field.as_deref(), Some("name"));
    }

    #[test]
    fn sqlx_error_message_is_redacted() {
        let error: AppError = sqlx::Error::RowNotFound.into();

        assert_eq!(error.code, "storage_error");
        assert!(!error.message.to_lowercase().contains("row"));
        assert!(!error.message.to_lowercase().contains("sqlite"));
    }

    #[test]
    fn io_error_message_is_redacted() {
        let error: AppError = std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "/Users/secret/workspace/documents/invoice.pdf",
        )
        .into();

        assert_eq!(error.code, "storage_error");
        assert_eq!(error.message, super::IO_ERROR_PUBLIC);
        assert!(!error.message.contains("/Users/"));
        assert!(!error.message.contains("invoice.pdf"));
    }

    #[test]
    fn unique_violation_flag_is_not_serialized_to_clients() {
        let mut error = AppError::storage(STORAGE_ERROR_PUBLIC);
        error.unique_violation = true;
        assert!(error.is_unique_violation());

        let serialized = serde_json::to_string(&error).expect("serialize");
        assert!(!serialized.contains("unique"));
    }
}
