use ufotofu_codec::{fuzz_relative_all, DecodingWentWrong};
use willow_data_model::Path;

const MCL: usize = 300;
const MCC: usize = 300;
const MPL: usize = 300;

fuzz_relative_all!(Path<MCL, MCC, MPL>; Path<MCL, MCC, MPL>; DecodingWentWrong; DecodingWentWrong);
