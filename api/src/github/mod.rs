pub mod v3;
pub mod v4;
mod utils;

#[derive(Copy, Clone, Debug)]
pub enum RequestCost {
    One,
    Custom(u64),
}

impl From<RequestCost> for u64 {
    fn from(rq: RequestCost) -> u64 {
        match rq {
            RequestCost::One => 1,
            RequestCost::Custom(x) => x
        }
    }
}
