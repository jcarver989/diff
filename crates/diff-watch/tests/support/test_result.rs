use std::error::Error;

pub type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;
