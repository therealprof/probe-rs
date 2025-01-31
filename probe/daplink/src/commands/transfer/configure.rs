use crate::commands::{
    Response,
    Category,
    Request,
    Result,
    Status,
};

/// The DAP_TransferConfigure Command sets parameters for DAP_Transfer and DAP_TransferBlock.
pub struct ConfigureRequest {
    /// Number of extra idle cycles after each transfer.
    pub idle_cycles: u8,
    /// Number of transfer retries after WAIT response.
    pub wait_retry: u16,
    /// Number of retries on reads with Value Match in DAP_Transfer. On value mismatch the Register is read again until its value matches or the Match Retry count exceeds.
    pub match_retry: u16,
}

impl Request for ConfigureRequest {
    const CATEGORY: Category = Category(0x04);

    fn to_bytes(&self, buffer: &mut [u8], offset: usize) -> Result<usize> {
        use scroll::Pwrite;

        buffer[offset] = self.idle_cycles;
        buffer.pwrite(self.wait_retry, offset + 1).expect("This is a bug. Please report it.");
        buffer.pwrite(self.match_retry, offset + 3).expect("This is a bug. Please report it.");
        Ok(5)
    }
}

pub struct ConfigureResponse(pub(crate) Status);

impl Response for ConfigureResponse {
    fn from_bytes(buffer: &[u8], offset: usize) -> Result<Self> {
        Ok(ConfigureResponse(Status::from_byte(buffer[offset])?))
    }
}