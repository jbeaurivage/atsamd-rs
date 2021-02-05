//! # Abstractions to setup and use the DMA controller
//!
//! # Initializing
//!
//! The DMAC should be initialized using the
//! [`DmaController::init`](DmaController::init) method. It will consume the
//! DMAC object generated by the PAC. By default, all four priority levels
//! will be enabled, but can be selectively enabled/disabled through the
//! `level_x_enabled` methods.
//!
//! # Splitting Channels
//!
//! Using the [`DmaController::split`](DmaController::split) method will return
//! a struct containing handles to individual channels.
//!
//! # Releasing the DMAC
//!
//! Using the [`DmaController::free`](DmaController::free) method will
//! deinitialize the DMAC and return the underlying PAC object.

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

/// Initialized DMA Controller
pub struct DmaController {
    dmac: DMAC,
}

impl DmaController {
    /// Return an immutable reference to the underlying DMAC object exposed by
    /// the PAC.
    pub unsafe fn dmac(&self) -> &DMAC {
        &self.dmac
    }

    /// Return a mutable reference to the underlying DMAC object exposed by the
    /// PAC.
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
        // TODO selectively enable priority levels
        // right now we blindly enable all priority levels
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

    /// Enable or disable priority level 0
    #[inline]
    pub fn level_0_enabled(&mut self, enabled: bool) {
        self.dmac.ctrl.modify(|_, w| w.lvlen0().bit(enabled));
    }

    /// Enable or disable priority level 1
    #[inline]
    pub fn level_1_enabled(&mut self, enabled: bool) {
        self.dmac.ctrl.modify(|_, w| w.lvlen1().bit(enabled));
    }

    /// Enable or disable priority level 2
    #[inline]
    pub fn level_2_enabled(&mut self, enabled: bool) {
        self.dmac.ctrl.modify(|_, w| w.lvlen2().bit(enabled));
    }

    /// Enable or disable priority level 3
    #[inline]
    pub fn level_3_enabled(&mut self, enabled: bool) {
        self.dmac.ctrl.modify(|_, w| w.lvlen3().bit(enabled));
    }

    /// Enable or disable Round-Robin Arbitration for priority level 0
    #[inline]
    pub fn level_0_round_robin(&mut self, enabled: bool) {
        self.dmac.prictrl0.modify(|_, w| w.rrlvlen0().bit(enabled));
    }

    /// Enable or disable Round-Robin Arbitration for priority level 1
    #[inline]
    pub fn level_1_round_robin(&mut self, enabled: bool) {
        self.dmac.prictrl0.modify(|_, w| w.rrlvlen1().bit(enabled));
    }

    /// Enable or disable Round-Robin Arbitration for priority level 2
    #[inline]
    pub fn level_2_round_robin(&mut self, enabled: bool) {
        self.dmac.prictrl0.modify(|_, w| w.rrlvlen2().bit(enabled));
    }

    /// Enable or disable Round-Robin Arbitration for priority level 3
    #[inline]
    pub fn level_3_round_robin(&mut self, enabled: bool) {
        self.dmac.prictrl0.modify(|_, w| w.rrlvlen3().bit(enabled));
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

    /// Split the DMAC into individual channels
    /// Struct generating individual handles to each DMA channel
    #[cfg(any(
        any(all(feature = "samd11", feature = "max-channels")),
        all(feature = "samd21", not(feature = "max-channels"))
    ))]
    pub fn split(&mut self) -> Channels {
        Channels(
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
        )
    }

    /// Split the DMAC into individual channels
    #[cfg(all(feature = "samd21", feature = "max-channels"))]
    pub fn split(&mut self) -> Channels {
        Channels(
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
        )
    }

    /// Split the DMAC into individual channels
    #[cfg(all(feature = "min-samd51g", not(feature = "max-channels")))]
    pub fn split(&mut self) -> Channels {
        Channels(
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
        )
    }

    /// Split the DMAC into individual channels
    #[cfg(all(feature = "min-samd51g", feature = "max-channels"))]
    pub fn split(&mut self) -> Channels {
        Channels(
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
            new_chan(),
        )
    }
}

/// Struct generating individual handles to each DMA channel
#[cfg(all(feature = "samd11", not(feature = "max-channels")))]
pub struct Channels(
    pub Channel<Uninitialized, 0>,
    pub Channel<Uninitialized, 1>,
    pub Channel<Uninitialized, 2>,
);

/// Struct generating individual handles to each DMA channel
#[cfg(any(
    any(all(feature = "samd11", feature = "max-channels")),
    all(feature = "samd21", not(feature = "max-channels"))
))]
pub struct Channels(
    pub Channel<Uninitialized, 0>,
    pub Channel<Uninitialized, 1>,
    pub Channel<Uninitialized, 2>,
    pub Channel<Uninitialized, 3>,
    pub Channel<Uninitialized, 4>,
    pub Channel<Uninitialized, 5>,
);

/// Struct generating individual handles to each DMA channel
#[cfg(all(feature = "samd21", feature = "max-channels"))]
pub struct Channels(
    pub Channel<Uninitialized, 0>,
    pub Channel<Uninitialized, 1>,
    pub Channel<Uninitialized, 2>,
    pub Channel<Uninitialized, 3>,
    pub Channel<Uninitialized, 4>,
    pub Channel<Uninitialized, 5>,
    pub Channel<Uninitialized, 6>,
    pub Channel<Uninitialized, 7>,
    pub Channel<Uninitialized, 8>,
    pub Channel<Uninitialized, 9>,
    pub Channel<Uninitialized, 10>,
    pub Channel<Uninitialized, 11>,
);

/// Struct generating individual handles to each DMA channel
#[cfg(all(feature = "min-samd51g", not(feature = "max-channels")))]
pub struct Channels(
    pub Channel<Uninitialized, 0>,
    pub Channel<Uninitialized, 1>,
    pub Channel<Uninitialized, 2>,
    pub Channel<Uninitialized, 3>,
    pub Channel<Uninitialized, 4>,
    pub Channel<Uninitialized, 5>,
    pub Channel<Uninitialized, 6>,
    pub Channel<Uninitialized, 7>,
    pub Channel<Uninitialized, 8>,
    pub Channel<Uninitialized, 9>,
    pub Channel<Uninitialized, 10>,
    pub Channel<Uninitialized, 11>,
    pub Channel<Uninitialized, 12>,
    pub Channel<Uninitialized, 13>,
    pub Channel<Uninitialized, 14>,
    pub Channel<Uninitialized, 15>,
);

/// Struct generating individual handles to each DMA channel
#[cfg(all(feature = "min-samd51g", feature = "max-channels"))]
pub struct Channels(
    pub Channel<Uninitialized, 0>,
    pub Channel<Uninitialized, 1>,
    pub Channel<Uninitialized, 2>,
    pub Channel<Uninitialized, 3>,
    pub Channel<Uninitialized, 4>,
    pub Channel<Uninitialized, 5>,
    pub Channel<Uninitialized, 6>,
    pub Channel<Uninitialized, 7>,
    pub Channel<Uninitialized, 8>,
    pub Channel<Uninitialized, 9>,
    pub Channel<Uninitialized, 10>,
    pub Channel<Uninitialized, 11>,
    pub Channel<Uninitialized, 12>,
    pub Channel<Uninitialized, 13>,
    pub Channel<Uninitialized, 14>,
    pub Channel<Uninitialized, 15>,
    pub Channel<Uninitialized, 16>,
    pub Channel<Uninitialized, 17>,
    pub Channel<Uninitialized, 18>,
    pub Channel<Uninitialized, 19>,
    pub Channel<Uninitialized, 20>,
    pub Channel<Uninitialized, 21>,
    pub Channel<Uninitialized, 22>,
    pub Channel<Uninitialized, 23>,
    pub Channel<Uninitialized, 24>,
    pub Channel<Uninitialized, 25>,
    pub Channel<Uninitialized, 26>,
    pub Channel<Uninitialized, 27>,
    pub Channel<Uninitialized, 28>,
    pub Channel<Uninitialized, 29>,
    pub Channel<Uninitialized, 30>,
    pub Channel<Uninitialized, 31>,
);
