// How to think about this from an expected value standpoint? Some way to discount them for the simulation?
// Likely much cheaper than needing to run the search multiple times -- could have an expectation mode and a truly random mode
pub enum Status {
    Prone,
    Threatened,
}
