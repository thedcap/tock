//! PWM driver for RP2040.

//use kernel::hil;
use kernel::utilities::cells::OptionalCell;
use kernel::utilities::registers::interfaces::Writeable;
use kernel::utilities::registers::{register_bitfields, ReadWrite, ReadOnly, WriteOnly};
use kernel::utilities::StaticRef;

use crate::clocks;

register_bitfields![u32,
    CSR [
        /// Enable PWM channel
        EN OFFSET(0) NUMBITS(1) [],
        /// Enable phase-correct modulation
        PH_CORRECT OFFSET(1) NUMBITS(1) [],
        /// Invert output A
        A_INV OFFSET(2) NUMBITS(1) [],
        /// Invert output B
        B_INV OFFSET(3) NUMBITS(1) [],
        /// PWM slice event selection for fractional clock divider
        /// Default value = FREE_RUNNING (always on)
        /// If the event is different from FREE_RUNNING, then pin B becomes
        /// an input pin
        DIVMOD OFFSET(4) NUMBITS(2) [
            /// Free-running counting at rate dictated by fractional divider
            FREE_RUNNING = 0,
            /// Fractional divider operation is gated by the PWM B pin
            B_HIGH = 1,
            /// Counter advances with each rising edge of the PWM B pin
            B_RISING = 2,
            /// Counter advances with each falling edge of the PWM B pin
            B_FALLING = 3
        ],
        /// Retard the phase of the counter by 1 count, while it is running
        /// Self-clearing. Write a 1, and poll until low. Counter must be running.
        PH_RET OFFSET(6) NUMBITS(1) [],
        /// Advance the phase of the counter by 1 count, while it is running
        /// Self clearing. Write a 1, and poll until low. Counter must be running.
        PH_ADV OFFSET(7) NUMBITS(1) []
    ],

    /// DIV register
    /// INT and FRAC form a fixed-point fractional number.
    /// Counting rate is system clock frequency divided by this number.
    /// Fractional division uses simple 1st-order sigma-delta.
    DIV [
        FRAC OFFSET(0) NUMBITS(4) [],
        INT OFFSET(4) NUMBITS(8) []
    ],

    /// Direct access to the PWM counter
    CTR [
        CTR OFFSET(0) NUMBITS(16) []
    ],

    /// Counter compare values
    CC [
        A OFFSET(0) NUMBITS(16) [],
        B OFFSET(16) NUMBITS(16) []
    ],

    /// Counter top value
    /// When the value of the counter reaches the top value, depending on the
    /// ph_correct value, the counter will either:
    /// + wrap to 0 if ph_correct == 0
    /// + it starts counting downward until it reaches 0 again if ph_correct == 0
    TOP [
        TOP OFFSET(0) NUMBITS(16) []
    ],

    /// Control multiple channels at once.
    /// Each bit controls one channel.
    CH [
        CH0 0,
        CH1 1,
        CH2 2,
        CH3 3,
        CH4 4,
        CH5 5,
        CH6 6,
        CH7 7
    ]
];

#[repr(C)]
struct Ch {
    /// Control and status register
    csr: ReadWrite<u32, CSR::Register>,
    /// Division register
    div: ReadWrite<u32, DIV::Register>,
    /// Direct access to the PWM counter register
    ctr: ReadWrite<u32, CTR::Register>,
    /// Counter compare values register
    cc: ReadWrite<u32, CC::Register>,
    /// Counter wrap value register
    top: ReadWrite<u32, TOP::Register>
}

#[repr(C)]
struct PwmRegisters {
    /// Channel registers
    // TODO: Remove hard coding of the number of channels
    // core::mem::variant_count::<ChannenlNumber>() can't be used since it is not stable
    ch: [Ch; 8],
    /// Enable register
    /// This register aliases the CSR_EN bits for all channels.
    /// Writing to this register allows multiple channels to be enabled or disabled
    /// or disables simultaneously, so they can run in perfect sync.
    en: ReadWrite<u32, CH::Register>,
    /// Raw interrupts register
    intr: WriteOnly<u32, CH::Register>,
    /// Interrupt enable register
    inte: ReadWrite<u32, CH::Register>,
    /// Interrupt force register
    intf: ReadWrite<u32, CH::Register>,
    /// Interrupt status after masking & forcing
    ints: ReadOnly<u32, CH::Register>
}

pub struct Pwm<'a> {
    registers: StaticRef<PwmRegisters>,
    clocks: OptionalCell<&'a clocks::Clocks>
}

#[derive(Clone, Copy)]
pub enum DivMode {
    FreeRunning,
    High,
    Rising,
    Falling
}

#[derive(Clone, Copy)]
pub enum ChannelNumber {
    Ch0,
    Ch1,
    Ch2,
    Ch3,
    Ch4,
    Ch5,
    Ch6,
    Ch7
}

pub struct PwmChannelConfiguration {
    en: bool,
    ph_correct: bool,
    a_inv: bool,
    b_inv: bool,
    divmode: DivMode,
    int: u8,
    frac: u8,
    cc_a: u16,
    cc_b: u16,
    top: u16,
}

impl PwmChannelConfiguration {
    /// Create a set of default values to use for configuring a PWM channel:
    /// + enabled = false
    /// + ph_correct = false
    /// + a_inv = false (no pin A polarity inversion)
    /// + b_inv = false (no pin B polarity inversion)
    /// + divmode = DivMode::FreeRunning (clock divider is always enabled)
    /// + int = 1 (integral part of the clock divider)
    /// + frac = 0 (fractional part of the clock divider)
    /// + cc_a = 0 (counter compare value for pin A)
    /// + cc_b = 0 (counter compare value for pin B)
    /// + top = u16::MAX (counter top value)
    pub fn default_config() -> Self {
        PwmChannelConfiguration {
            en: false,
            ph_correct: false,
            a_inv: false,
            b_inv: false,
            divmode: DivMode::FreeRunning,
            int: 1,
            frac: 0,
            cc_a: 0,
            cc_b: 0,
            top: u16::MAX
        }
    }

    // enable == false ==> disable channel
    // enable == true ==> enable channel
    pub fn set_enabled(&mut self, enable: bool) {
        self.en = enable;
    }

    // ph_correct == false ==> trailing-edge modulation
    // ph_correct == true ==> phase-correct modulation
    pub fn set_ph_correct(&mut self, ph_correct: bool) {
        self.ph_correct = ph_correct;
    }

    // a_inv == true ==> invert polarity for pin A
    // b_inv == true ==> invert polarity for pin B
    pub fn set_invert_polarity(&mut self, a_inv: bool, b_inv: bool) {
        self.a_inv = a_inv;
        self.b_inv = b_inv;
    }

    // divmode == FreeRunning ==> always enable clock divider
    // divmode == High ==> enable clock divider when pin B is high
    // divmode == Rising ==> enable clock divider when pin B is rising
    // divmode == Falling ==> enable clock divider when pin B is falling
    pub fn set_div_mode(&mut self, divmode: DivMode) {
        self.divmode = divmode;
    }

    // RP 2040 uses a 8.4 fractional clock divider
    // The minimum value of the divider is   1 (int) +  0 / 16 (frac)
    // The maximum value of the divider is 255 (int) + 15 / 16 (frac)
    pub fn set_divider_int_frac(&mut self, int: u8, frac: u8) {
        // No need to check the upper bound, since the int parameter is u8
        assert!(int >= 1);
        // No need to check the lower bound, since the frac parameter is u8
        assert!(frac <= 15);
        self.int = int;
        self.frac = frac;
    }

    // Set compare values
    // If counter value < compare value A ==> pin A high
    // If couter value < compare value B ==> pin B high (if divmode == FreeRunning)
    pub fn set_compare_values(&mut self, cc_a: u16, cc_b: u16) {
        self.cc_a = cc_a;
        self.cc_b = cc_b;
    }

    // Set counter top value
    pub fn set_top_value(&mut self, top: u16) {
        self.top = top;
    }
}

const PWM_BASE: StaticRef<PwmRegisters> =
    unsafe { StaticRef::new(0x40050000 as *const PwmRegisters) };

impl<'a> Pwm<'a> {
    pub fn new() -> Self {
        Self {
            registers: PWM_BASE,
            clocks: OptionalCell::empty()
        }
    }

    // enable == false ==> disable channel
    // enable == true ==> enable channel
    pub fn set_enabled(&self, channel_number: ChannelNumber, enable: bool) {
        self.registers.ch[channel_number as usize].csr.write(match enable {
            true => CSR::EN::SET,
            false => CSR::EN::CLEAR
        });
    }

    // ph_correct == false ==> trailing-edge modulation
    // ph_correct == true ==> phase-correct modulation
    pub fn set_ph_correct(&self, channel_number: ChannelNumber, ph_correct: bool) {
        self.registers.ch[channel_number as usize].csr.write(match ph_correct {
            true => CSR::PH_CORRECT::SET,
            false => CSR::PH_CORRECT::CLEAR
        });
    }

    // a_inv == true ==> invert polarity for pin A
    // b_inv == true ==> invert polarity for pin B
    pub fn set_invert_polarity(&self, channel_number: ChannelNumber, a_inv: bool, b_inv: bool) {
        self.registers.ch[channel_number as usize].csr.write(match a_inv {
            true => CSR::A_INV::SET,
            false => CSR::A_INV::CLEAR
        });
        self.registers.ch[channel_number as usize].csr.write(match b_inv {
            true => CSR::B_INV::SET,
            false => CSR::B_INV::CLEAR
        });
    }

    // divmode == FreeRunning ==> always enable clock divider
    // divmode == High ==> enable clock divider when pin B is high
    // divmode == Rising ==> enable clock divider when pin B is rising
    // divmode == Falling ==> enable clock divider when pin B is falling
    pub fn set_div_mode(&self, channel_number: ChannelNumber, divmode: DivMode) {
        self.registers.ch[channel_number as usize].csr.write(match divmode {
            DivMode::FreeRunning => CSR::DIVMOD::FREE_RUNNING,
            DivMode::High => CSR::DIVMOD::B_HIGH,
            DivMode::Rising => CSR::DIVMOD::B_RISING,
            DivMode::Falling => CSR::DIVMOD::B_FALLING
        });
    }

    // RP 2040 uses a 8.4 fractional clock divider
    // The minimum value of the divider is   1 (int) +  0 / 16 (frac)
    // The maximum value of the divider is 255 (int) + 15 / 16 (frac)
    pub fn set_divider_int_frac(&self, channel_number: ChannelNumber, int: u8, frac: u8) {
        // No need to check the upper bound, since the int parameter is u8
        assert!(int >= 1);
        // No need to check the lower bound, since the frac parameter is u8
        assert!(frac <= 15);
        self.registers.ch[channel_number as usize].div.write(DIV::INT.val(int as u32));
        self.registers.ch[channel_number as usize].div.write(DIV::FRAC.val(frac as u32));
    }

    // Set compare values
    // If counter value < compare value A ==> pin A high
    // If couter value < compare value B ==> pin B high (if divmode == FreeRunning)
    pub fn set_compare_values(&self, channel_number: ChannelNumber, cc_a: u16, cc_b: u16) {
        self.registers.ch[channel_number as usize].cc.write(CC::A.val(cc_a as u32));
        self.registers.ch[channel_number as usize].cc.write(CC::B.val(cc_b as u32));
    }

    // Set counter top value
    pub fn set_top(&self, channel_number: ChannelNumber, top: u16) {
        self.registers.ch[channel_number as usize].top.write(TOP::TOP.val(top as u32));
    }

    pub fn configure_channel(&self, channel_number: ChannelNumber, config: &PwmChannelConfiguration) {
        self.set_enabled(channel_number, config.en);
        self.set_ph_correct(channel_number, config.ph_correct);
        self.set_invert_polarity(channel_number, config.a_inv, config.b_inv);
        self.set_div_mode(channel_number, config.divmode);
        self.set_divider_int_frac(channel_number, config.int, config.frac);
        self.set_compare_values(channel_number, config.cc_a, config.cc_b);
        self.set_top(channel_number, config.top);
    }

    pub fn init(&self) {
        let channel_numbers = [
            ChannelNumber::Ch0,
            ChannelNumber::Ch1,
            ChannelNumber::Ch2,
            ChannelNumber::Ch3,
            ChannelNumber::Ch4,
            ChannelNumber::Ch5,
            ChannelNumber::Ch6,
            ChannelNumber::Ch7,
        ];
        let default_config = PwmChannelConfiguration::default_config();
        for channel_number in channel_numbers {
            self.configure_channel(channel_number, &default_config);
        }
    }

    pub fn set_clocks(&self, clocks: &'a clocks::Clocks) {
        self.clocks.set(clocks);
    }
}
