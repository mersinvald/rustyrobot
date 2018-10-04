macro_rules! raise {
    ($error:expr) => {
        return Err($error.into());
    };
}
