/// 문자열 id 값 객체를 만든다. Java의 `record XxxId(String value)`에 대응한다.
macro_rules! string_id {
    ($name:ident, $required_message:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            pub fn of(value: &str) -> $crate::error::AppResult<Self> {
                if value.trim().is_empty() {
                    return Err($crate::error::AppError::InvalidArgument(
                        $required_message.to_string(),
                    ));
                }
                Ok(Self(value.to_string()))
            }

            pub fn new_id() -> Self {
                Self(uuid::Uuid::new_v4().to_string())
            }

            pub fn value(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}
