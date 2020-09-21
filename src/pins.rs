use derive_try_from_primitive::TryFromPrimitive;
#[repr(u8)]
#[derive(TryFromPrimitive, Copy, Clone, Debug, Eq, PartialEq)]
pub enum Pin {
    Phy7 = 249,
    Phy8 = 240,
    Phy10 = 241,
    Phy11 = 247,
    Phy12 = 238,
    Phy13 = 239,
    Phy15 = 237,
    Phy16 = 236,
    Phy18 = 233,
    Phy19 = 235,
    Phy21 = 232,
    Phy22 = 231,
    Phy23 = 230,
    Phy24 = 229,
    Phy26 = 225,
    Phy27 = 207,
    Phy28 = 208,
    Phy29 = 228,
    Phy31 = 219,
    Phy32 = 224,
    Phy33 = 234,
    Phy35 = 214,
    Phy36 = 218,
}
