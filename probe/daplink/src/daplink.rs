use coresight::debug_port::DPRegister;
use coresight::dp_access::DPAccess;
use coresight::dp_access::DebugPort;
use probe::debug_probe::{
    DebugProbe,
    DebugProbeError,
    DebugProbeInfo,
    DAPAccess,
};
use probe::debug_probe;
use probe::protocol::WireProtocol;

use log::{error, debug, info};

use crate::commands::{
    Status,
    general::{
        connect::{
            ConnectRequest,
            ConnectResponse,
        },
        disconnect::{
            DisconnectRequest,
            DisconnectResponse,
        },
        reset::{
            ResetRequest,
            ResetResponse,
        }
    },
    transfer::{
        transfer::{
            TransferRequest,
            TransferResponse,
            InnerTransferRequest,
            Ack,
            Port,
            RW,
        },
        configure::{
            ConfigureRequest,
            ConfigureResponse,
        }
    },
    swj::{
        clock::{
            SWJClockRequest,
            SWJClockResponse,
        },
        sequence::{
            SequenceRequest,
            SequenceResponse,
        },
    },
    swd,
};

pub struct DAPLink {
    pub device: hidapi::HidDevice,
    _hw_version: u8,
    _jtag_version: u8,
    _protocol: WireProtocol,
}


impl DAPLink {
    pub fn new_from_device(device: hidapi::HidDevice) -> Self {
        Self {
            device,
            _hw_version: 0,
            _jtag_version: 0,
            _protocol: WireProtocol::Swd,
        }
    }

    fn set_swj_clock(&self, clock: u32) -> Result<(), DebugProbeError> {
        use crate::commands::Error;
        crate::commands::send_command::<SWJClockRequest, SWJClockResponse>(
            &self.device,
            SWJClockRequest(clock),
        )
        .and_then(|v| match v {
            SWJClockResponse(Status::DAPOk) => Ok(()),
            SWJClockResponse(Status::DAPError) => Err(Error::DAPError),
        })?;
        Ok(())
    }

    fn transfer_configure(&self, request: ConfigureRequest) -> Result<(), DebugProbeError> {
        use crate::commands::Error;
        crate::commands::send_command::<ConfigureRequest, ConfigureResponse>(
            &self.device,
            request,
        )
        .and_then(|v| match v {
            ConfigureResponse(Status::DAPOk) => Ok(()),
            ConfigureResponse(Status::DAPError) => Err(Error::DAPError),
        })?;
        Ok(())
    }

    fn configure_swd(&self, request: swd::configure::ConfigureRequest) -> Result<(), DebugProbeError> {
        use crate::commands::Error;


        crate::commands::send_command::<swd::configure::ConfigureRequest, swd::configure::ConfigureResponse>(
            &self.device, 
            request
        )
        .and_then(|v| match v {
            swd::configure::ConfigureResponse(Status::DAPOk) => Ok(()),
            swd::configure::ConfigureResponse(Status::DAPError) => Err(Error::DAPError),
        })?;
        Ok(())
    }

    fn send_swj_sequences(&self, request: SequenceRequest) -> Result<(), DebugProbeError> {
        /* 12 38 FF FF FF FF FF FF FF -> 12 00 // SWJ Sequence
        12 10 9E E7 -> 12 00 // SWJ Sequence
        12 38 FF FF FF FF FF FF FF -> 12 00 // SWJ Sequence */
        //let sequence_1 = SequenceRequest::new(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        use crate::commands::Error;


        crate::commands::send_command::<SequenceRequest, SequenceResponse>(
            &self.device, 
            request
        )
        .and_then(|v| match v {
            SequenceResponse(Status::DAPOk) => Ok(()),
            SequenceResponse(Status::DAPError) => Err(Error::DAPError),
        })?;
        Ok(())
    }
}

impl<P: DebugPort, R: DPRegister<P>> DPAccess<P,R> for DAPLink {
    type Error = DebugProbeError;

    fn read_dp_register(&mut self, _port: &P) -> Result<R, Self::Error> {
        debug!("Reading DP register {}", R::NAME);
        let result = self.read_register(debug_probe::Port::DebugPort, u16::from(R::ADDRESS))?;

        debug!("Read    DP register {}, value=0x{:08x}", R::NAME, result);

        Ok(result.into())
    }

    fn write_dp_register(&mut self, _port: &P, register: R) -> Result<(), Self::Error> {
        let value = register.into();

        debug!("Writing DP register {}, value=0x{:08x}", R::NAME, value);
        self.write_register(debug_probe::Port::DebugPort, u16::from(R::ADDRESS), value)
    }

}

impl DebugProbe for DAPLink {
    fn new_from_probe_info(info: &DebugProbeInfo) -> Result<Box<Self>, DebugProbeError> where Self: Sized {
        if let Some(serial_number) = &info.serial_number {
            Ok(Box::new(Self::new_from_device(
                hidapi::HidApi::new()
                    .map_err(|_| DebugProbeError::ProbeCouldNotBeCreated)?
                    .open_serial(info.vendor_id, info.product_id, &serial_number)
                    .map_err(|_| DebugProbeError::ProbeCouldNotBeCreated)?
            )))
        } else {
            Ok(Box::new(Self::new_from_device(
                hidapi::HidApi::new()
                    .map_err(|_| DebugProbeError::ProbeCouldNotBeCreated)?
                    .open(info.vendor_id, info.product_id)
                    .map_err(|_| DebugProbeError::ProbeCouldNotBeCreated)?
            )))
        }
    }

    fn get_name(&self) -> &str {
        "DAPLink"
    }

    /// Enters debug mode.
    fn attach(&mut self, protocol: Option<WireProtocol>) -> Result<WireProtocol, DebugProbeError> {
        use crate::commands::Error;

        info!("Attaching to target system");
        self.set_swj_clock(1_000_000)?;

        let protocol = if let Some(protocol) = protocol {
            match protocol {
                WireProtocol::Swd => ConnectRequest::UseSWD,
                WireProtocol::Jtag => ConnectRequest::UseJTAG,
            }
        } else {
            ConnectRequest::UseDefaultPort
        };

        let result = crate::commands::send_command(&self.device, protocol)
            .and_then(|v| match v {
                ConnectResponse::SuccessfulInitForSWD => Ok(WireProtocol::Swd),
                ConnectResponse::SuccessfulInitForJTAG => Ok(WireProtocol::Jtag),
                ConnectResponse::InitFailed => Err(Error::DAPError),
            })?;

        self.set_swj_clock(1_000_000)?;

        self.transfer_configure(ConfigureRequest {
            idle_cycles: 0,
            wait_retry: 80,
            match_retry: 0,
        })?;

        self.configure_swd(swd::configure::ConfigureRequest {})?;

        self.send_swj_sequences(SequenceRequest::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]).unwrap())?;

        self.send_swj_sequences(SequenceRequest::new(&[0x9e, 0xe7]).unwrap())?;

        self.send_swj_sequences(SequenceRequest::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]).unwrap())?;

        self.send_swj_sequences(SequenceRequest::new(&[0x00]).unwrap())?;

        use coresight::debug_port::{
            Ctrl,
            Select,
            Abort,
            DPIDR,
            DebugPortId,
            DPv1,
        };

        // assume a dpv1 port for now

        let port = DPv1{};

        let dp_id: DPIDR = self.read_dp_register(&port)?;

        let dp_id: DebugPortId = dp_id.into();

        info!("Debug Port Version:  {:x?}", dp_id.version);
        info!("Debug Port Designer: {}", dp_id.designer.get().unwrap_or("Unknown"));

        let mut abort_reg = Abort(0);
        abort_reg.set_orunerrclr(true);
        abort_reg.set_wderrclr(true);
        abort_reg.set_stkerrclr(true);
        abort_reg.set_stkcmpclr(true);

        self.write_dp_register(&port, abort_reg)?; // clear errors 


        let mut select_reg = Select(0);
        select_reg.set_dp_bank_sel(0);

        self.write_dp_register(&port, select_reg)?; // select DBPANK 0

        let mut ctrl_reg = Ctrl::default();

        ctrl_reg.set_csyspwrupreq(true);
        ctrl_reg.set_cdbgpwrupreq(true);

        debug!("Requesting debug power");

        self.write_dp_register(&port, ctrl_reg)?; // CSYSPWRUPREQ, CDBGPWRUPREQ

        // TODO: Check return value if power up was ok
        let ctrl_reg: Ctrl = self.read_dp_register(&port)?; 

        if !(ctrl_reg.csyspwrupack() && ctrl_reg.cdbgpwrupack()) {
            error!("Debug power request failed");
            return Err(DebugProbeError::TargetPowerUpFailed);
        }

        info!("Succesfully attached to system and entered debug mode");

        Ok(result)
    }

    /// Leave debug mode.
    fn detach(&mut self) -> Result<(), DebugProbeError> {
        crate::commands::send_command(
            &self.device,
            DisconnectRequest {},
        )
        .map_err(|_| DebugProbeError::USBError)
        .and_then(|v: DisconnectResponse| match v {
            DisconnectResponse(Status::DAPOk) => Ok(()),
            DisconnectResponse(Status::DAPError) => Err(DebugProbeError::UnknownError)
        })
    }

    /// Asserts the nRESET pin.
    fn target_reset(&mut self) -> Result<(), DebugProbeError> {
        crate::commands::send_command(
            &self.device,
            ResetRequest
        )
        .map(|v: ResetResponse| {
            println!("Target reset response: {:?}", v);
            ()
        })?;
        Ok(())
    }
}

impl DAPAccess for DAPLink {
    /// Reads the DAP register on the specified port and address.
    fn read_register(&mut self, port: debug_probe::Port, addr: u16) -> Result<u32, DebugProbeError> {
        let port = match port {
            debug_probe::Port::DebugPort => Port::DP,
            debug_probe::Port::AccessPort(_) => Port::AP,
        };

        crate::commands::send_command::<TransferRequest, TransferResponse>(
            &self.device,
            TransferRequest::new(
                InnerTransferRequest::new(port, RW::R, addr as u8),
                0
            )
        )
        .map_err(|_| DebugProbeError::UnknownError)
        .and_then(|v| {
            if v.transfer_count == 1 {
                if v.transfer_response.protocol_error {
                    Err(DebugProbeError::USBError)
                } else {
                    match v.transfer_response.ack {
                        Ack::Ok => Ok(v.transfer_data),
                        _ => Err(DebugProbeError::UnknownError)
                    }
                }
            } else {
                Err(DebugProbeError::UnknownError)
            }
        })
    }

    /// Writes a value to the DAP register on the specified port and address.
    fn write_register(&mut self, port: debug_probe::Port, addr: u16, value: u32) -> Result<(), DebugProbeError> {
        let port = match port {
            debug_probe::Port::DebugPort => Port::DP,
            debug_probe::Port::AccessPort(_) => Port::AP,
        };

        crate::commands::send_command::<TransferRequest, TransferResponse>(
            &self.device,
            TransferRequest::new(
                InnerTransferRequest::new(port, RW::W, addr as u8),
                value
            )
        )
        .map_err(|_| DebugProbeError::UnknownError)
        .and_then(|v| {
            if v.transfer_count == 1 {
                if v.transfer_response.protocol_error {
                    Err(DebugProbeError::USBError)
                } else {
                    match v.transfer_response.ack {
                        Ack::Ok => Ok(()),
                        _ => Err(DebugProbeError::UnknownError)
                    }
                }
            } else {
                Err(DebugProbeError::UnknownError)
            }
        })
    }
}

impl Drop for DAPLink {
    fn drop(&mut self) {
        debug!("Detaching from DAPLink");
        // We ignore the error case as we can't do much about it anyways.
        let _ = self.detach();
    }
}