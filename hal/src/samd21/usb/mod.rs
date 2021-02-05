//! USB Device support

#[cfg(not(feature = "unproven"))]
use crate::gpio;

#[cfg(feature = "unproven")]
use crate::gpio::v2::{AlternateG, Pin, PA23, PA24, PA25};

pub use usb_device;

mod bus;
pub use self::bus::UsbBus;

mod devicedesc;
use self::devicedesc::Descriptors;

/// Emit SOF at 1Khz on this pin when configured as function G
#[cfg(feature = "unproven")]
pub type SofPad = Pin<PA23, AlternateG>;
#[cfg(not(feature = "unproven"))]
pub type SofPad = gpio::Pa23<gpio::PfG>;

/// USB D- is connected here
#[cfg(feature = "unproven")]
pub type DmPad = Pin<PA24, AlternateG>;
#[cfg(not(feature = "unproven"))]
pub type DmPad = gpio::Pa24<gpio::PfG>;

/// USB D+ is connected here
#[cfg(feature = "unproven")]
pub type DpPad = Pin<PA25, AlternateG>;
#[cfg(not(feature = "unproven"))]
pub type DpPad = gpio::Pa25<gpio::PfG>;
