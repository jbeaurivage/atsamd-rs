//! # Abstractions to setup and use the DMA controller
//!
//! # Initializing
//!
//! The DMAC should be initialized using the
//! [`DmaController::init`] method. It will consume the
//! DMAC object generated by the PAC. By default, all four priority levels
//! will be enabled, but can be selectively enabled/disabled through the
//! [`DmaController::enable_levels`] ansd [`DmaController::disable_levels`]
//! methods.
//!
//! # Splitting Channels
//!
//! Using the [`DmaController::split`] method will return
//! a struct containing handles to individual channels.
//!
//! # Releasing the DMAC
//!
//! Using the [`DmaController::free`] method will
//! deinitialize the DMAC and return the underlying PAC object.

use modular_bitfield::prelude::*;
use paste::paste;
use seq_macro::seq;

#[cfg(any(feature = "samd11", feature = "samd21"))]
pub use crate::target_device::dmac::chctrlb::{
    LVL_A as PriorityLevel, TRIGACT_A as TriggerAction, TRIGSRC_A as TriggerSource,
};

#[cfg(feature = "min-samd51g")]
pub use crate::target_device::dmac::channel::{
    chctrla::{
        BURSTLEN_A as BurstLength, THRESHOLD_A as FifoThreshold, TRIGACT_A as TriggerAction,
        TRIGSRC_A as TriggerSource,
    },
    chprilvl::PRILVL_A as PriorityLevel,
};

use super::{
    channel::{new_chan, Channel, Uninitialized},
    DESCRIPTOR_SECTION, WRITEBACK,
};
use crate::target_device::{DMAC, PM};

/// Trait representing a DMA channel ID
pub trait ChId {
    const U8: u8;
    const USIZE: usize;
}

macro_rules! define_channels_struct {
    ($num_channels:literal) => {
        seq!(N in 0..$num_channels {
            paste! {
                #(
                    /// Type alias for a channel number
                    pub struct [<Ch N>];

                    impl ChId for [<Ch N>] {
                        const U8: u8 = N;
                        const USIZE: usize = N;
                    }
                )*

                /// Struct generating individual handles to each DMA channel
                pub struct Channels(
                    #(
                        pub Channel<[<Ch N>], Uninitialized>,
                    )*
                );
            }
        });
    };
}

with_num_channels!(define_channels_struct);

/// Initialized DMA Controller
pub struct DmaController {
    dmac: DMAC,
}

#[bitfield]
#[repr(u16)]
pub struct PriorityLevelMask {
    #[skip]
    _reserved: B8,
    /// Level 0
    #[allow(dead_code)]
    level0: bool,
    /// Level 1
    #[allow(dead_code)]
    level1: bool,
    /// Level 2
    #[allow(dead_code)]
    level2: bool,
    /// Level 3
    #[allow(dead_code)]
    level3: bool,
    #[skip]
    _reserved: B4,
}

#[bitfield]
#[repr(u32)]
pub struct RoundRobinMask {
    #[skip]
    _reserved: B7,
    /// Level 0
    #[allow(dead_code)]
    level0: bool,
    #[skip]
    _reserved: B7,
    /// Level 1
    #[allow(dead_code)]
    level1: bool,
    #[skip]
    _reserved: B7,
    /// Level 2
    #[allow(dead_code)]
    level2: bool,
    #[skip]
    _reserved: B7,
    /// Level 3
    #[allow(dead_code)]
    level3: bool,
}

impl DmaController {
    /// Return an immutable reference to the underlying DMAC object exposed by
    /// the PAC.
    ///
    /// # Safety
    ///
    /// This function is unsafe because `DmaController` may expect certain
    /// registers to retain a configuration. Messing with that configuration may
    /// be unsafe.
    pub unsafe fn dmac(&self) -> &DMAC {
        &self.dmac
    }

    /// Return a mutable reference to the underlying DMAC object exposed by the
    /// PAC.
    ///
    /// # Safety
    ///
    /// This function is unsafe because `DmaController` may expect certain
    /// registers to retain a configuration. Messing with that configuration may
    /// be unsafe.
    pub unsafe fn dmac_mut(&mut self) -> &mut DMAC {
        &mut self.dmac
    }

    /// Initialize the DMAC and return a DmaController object useable by
    /// [`Transfer`](super::transfer::Transfer)'s. By default, all
    /// priority levels are enabled unless subsequently disabled using the
    /// `level_x_enabled` methods.
    pub fn init(mut dmac: DMAC, _pm: &mut PM) -> Self {
        // ----- Initialize clocking ----- //
        #[cfg(any(feature = "samd11", feature = "samd21"))]
        {
            // Enable clocking
            _pm.ahbmask.modify(|_, w| w.dmac_().set_bit());
            _pm.apbbmask.modify(|_, w| w.dmac_().set_bit());
        }

        Self::swreset(&mut dmac);

        // SAFETY this is safe because we write a whole u32 to 32-bit registers,
        // and the descriptor array addesses will never change since they are static.
        // We just need to ensure the writeback and descriptor_section addresses
        // are valid.
        unsafe {
            dmac.baseaddr
                .write(|w| w.baseaddr().bits(DESCRIPTOR_SECTION.as_ptr() as u32));
            dmac.wrbaddr
                .write(|w| w.wrbaddr().bits(WRITEBACK.as_ptr() as u32));
        }

        // ----- Select priority levels ----- //
        dmac.ctrl.modify(|_, w| {
            w.lvlen3().set_bit();
            w.lvlen2().set_bit();
            w.lvlen1().set_bit();
            w.lvlen0().set_bit()
        });

        // Enable DMA controller
        dmac.ctrl.modify(|_, w| w.dmaenable().set_bit());

        Self { dmac }
    }

    /// Enable multiple priority levels simultaneously
    #[inline]
    pub fn enable_levels(&mut self, mask: PriorityLevelMask) {
        // SAFETY This is safe because the use of bitfields ensures that only the
        // LVLENx bits are written to. The fact that we are given a mask means we need
        // to do the bit-level setting ourselves.
            let mask: u16 = mask.into();
        unsafe {
            self.dmac.ctrl.modify(|r, w| w.bits(r.bits() | mask));
        }
    }

    /// Disable multiple priority levels simultaneously
    #[inline]
    pub fn disable_levels(&mut self, mask: PriorityLevelMask) {
        // SAFETY This is safe because the use of bitfields ensures that only the
        // LVLENx bits are written to. The fact that we are given a mask means we need
        // to do the bit-level clearing ourselves.
            let mask: u16 = mask.into(); 
        unsafe {
            self.dmac.ctrl.modify(|r, w| w.bits(r.bits() & !mask));
        }
    }

    /// Enable round-robin arbitration for multiple priority levels
    /// simultaneously
    #[inline]
    pub fn round_robin_arbitration(&mut self, mask: RoundRobinMask) {
        // SAFETY This is safe because the use of bitfields ensures that only the
        // RRLVLENx bits are written to. The fact that we are given a mask means we need
        // to do the bit-level setting ourselves.
            let mask: u32 = mask.into();
        unsafe {
            self.dmac.prictrl0.modify(|r, w| w.bits(r.bits() | mask));
        }
    }

    /// Disable round-robin arbitration (ie, enable static priorities) for
    /// multiple priority levels simultaneously
    #[inline]
    pub fn static_arbitration(&mut self, mask: RoundRobinMask) {
        // SAFETY This is safe because the use of bitfields ensures that only the
        // RRLVLENx bits are written to. The fact that we are given a mask means we need
        // to do the bit-level clearing ourselves.
            let mask: u32 = mask.into();
        unsafe {
            self.dmac.prictrl0.modify(|r, w| w.bits(r.bits() & !mask));
        }
    }

    /// Release the DMAC and return the register block
    pub fn free(mut self, _pm: &mut PM) -> DMAC {
        self.dmac.ctrl.modify(|_, w| w.dmaenable().clear_bit());

        Self::swreset(&mut self.dmac);

        #[cfg(any(feature = "samd11", feature = "samd21"))]
        {
            // Disable the DMAC clocking
            _pm.apbbmask.modify(|_, w| w.dmac_().clear_bit());
            _pm.ahbmask.modify(|_, w| w.dmac_().clear_bit());
        }

        // Release the DMAC
        self.dmac
    }

    /// Issue a software reset to the DMAC and wait for reset to complete
    #[inline]
    fn swreset(dmac: &mut DMAC) {
        dmac.ctrl.modify(|_, w| w.swrst().set_bit());
        while dmac.ctrl.read().swrst().bit_is_set() {}
    }

    /// Split the DMAC into individual channels
    #[cfg(all(feature = "samd11", not(feature = "max-channels")))]
    pub fn split(&mut self) -> Channels {
        Channels(new_chan(), new_chan(), new_chan())
    }
}

macro_rules! define_split {
    ($num_channels:literal) => {
        seq!(N in 0..$num_channels {
            /// Split the DMAC into individual channels
            pub fn split(&mut self) -> Channels {
                Channels(
                    #(
                        new_chan(core::marker::PhantomData),
                    )*
                )
            }
        });
    };
}

impl DmaController {
    with_num_channels!(define_split);
}
