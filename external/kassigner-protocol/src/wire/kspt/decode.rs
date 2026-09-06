use super::{
    DecodeError, DecodeLimits, DecodedEnvelope, Derivation, Global, Input, Ms45Derivation, Output, Signature,
    WireError, ALLOWED_FLAGS, COVENANT_MARKER, GENERATION_CURRENT, INPUT_DERIVATION_MARKER,
    MAGIC, MS45_INPUT_MARKER, MS45_OUTPUT_MARKER, NETWORK_MARKER, OUTPUT_DERIVATION_MARKER,
    STEALTH_MARKER, valid_derivation, valid_ms45, valid_network, valid_sighash,
};
use super::io::Reader;

pub trait DecodeSink {
    type Error;
    fn global(&mut self, value: Global<'_>) -> Result<(), Self::Error>;
    fn input(&mut self, index: u32, value: Input<'_>, signature_count: u8) -> Result<(), Self::Error>;
    fn signature(&mut self, input: u32, slot: u8, value: Signature) -> Result<(), Self::Error>;
    fn redeem(&mut self, input: u32, value: &[u8]) -> Result<(), Self::Error>;
    fn output(&mut self, index: u8, value: Output<'_>) -> Result<(), Self::Error>;
    fn network(&mut self, code: u8) -> Result<(), Self::Error>;
    fn stealth(&mut self, tweak: [u8; 32]) -> Result<(), Self::Error>;
    fn input_derivation(&mut self, input: u8, value: Derivation) -> Result<(), Self::Error>;
    fn output_derivation(&mut self, output: u8, value: Derivation) -> Result<(), Self::Error>;
    fn input_ms45(&mut self, input: u8, value: Ms45Derivation) -> Result<(), Self::Error>;
    fn output_ms45(&mut self, output: u8, value: Ms45Derivation) -> Result<(), Self::Error>;
    fn covenant(&mut self, output: u8, authorizing_input: u16, id: [u8; 32]) -> Result<(), Self::Error>;
}


/// Validate a compact KSPT using the canonical grammar without constructing a consumer model.
pub fn validate(data: &[u8]) -> Result<DecodedEnvelope, WireError> {
    let mut sink = NullSink;
    match decode(data, &mut sink) {
        Ok(envelope) => Ok(envelope),
        Err(DecodeError::Wire(error)) => Err(error),
        Err(DecodeError::Sink(error)) => match error {},
    }
}

struct NullSink;
impl DecodeSink for NullSink {
    type Error = core::convert::Infallible;
    fn global(&mut self, value: Global<'_>) -> Result<(), Self::Error> { let _ = value; Ok(()) }
    fn input(&mut self, index: u32, value: Input<'_>, signature_count: u8) -> Result<(), Self::Error> { let _ = (index, value, signature_count); Ok(()) }
    fn signature(&mut self, input: u32, slot: u8, value: Signature) -> Result<(), Self::Error> { let _ = (input, slot, value); Ok(()) }
    fn redeem(&mut self, input: u32, value: &[u8]) -> Result<(), Self::Error> { let _ = (input, value); Ok(()) }
    fn output(&mut self, index: u8, value: Output<'_>) -> Result<(), Self::Error> { let _ = (index, value); Ok(()) }
    fn network(&mut self, code: u8) -> Result<(), Self::Error> { let _ = code; Ok(()) }
    fn stealth(&mut self, tweak: [u8; 32]) -> Result<(), Self::Error> { let _ = tweak; Ok(()) }
    fn input_derivation(&mut self, input: u8, value: Derivation) -> Result<(), Self::Error> { let _ = (input, value); Ok(()) }
    fn output_derivation(&mut self, output: u8, value: Derivation) -> Result<(), Self::Error> { let _ = (output, value); Ok(()) }
    fn input_ms45(&mut self, input: u8, value: Ms45Derivation) -> Result<(), Self::Error> { let _ = (input, value); Ok(()) }
    fn output_ms45(&mut self, output: u8, value: Ms45Derivation) -> Result<(), Self::Error> { let _ = (output, value); Ok(()) }
    fn covenant(&mut self, output: u8, authorizing_input: u16, id: [u8; 32]) -> Result<(), Self::Error> { let _ = (output, authorizing_input, id); Ok(()) }
}

pub fn decode<S: DecodeSink>(data: &[u8], sink: &mut S) -> Result<DecodedEnvelope, DecodeError<S::Error>> {
    decode_with_limits(data, sink, DecodeLimits::unbounded())
}

pub fn decode_with_limits<S: DecodeSink>(
    data: &[u8],
    sink: &mut S,
    limits: DecodeLimits,
) -> Result<DecodedEnvelope, DecodeError<S::Error>> {
    let mut reader = Reader::new(data);
    let flags = read_header(&mut reader)?;
    let prefix = read_global_prefix(&mut reader, limits)?;
    let global = read_global(flags, prefix, &mut reader)?;
    validate_input_capacity(global.input_count, reader.remaining())?;
    sink.global(global).map_err(DecodeError::Sink)?;
    read_inputs(&mut reader, global.input_count, sink)?;
    read_outputs(&mut reader, global.output_count, sink)?;
    read_trailers(&mut reader, global.input_count, global.output_count, sink)?;
    if reader.remaining() != 0 { return Err(WireError::TrailingData.into()); }
    Ok(DecodedEnvelope { flags })
}

fn read_header(reader: &mut Reader<'_>) -> Result<u8, WireError> {
    if reader.bytes(4)? != MAGIC { return Err(WireError::InvalidMagic); }
    if reader.u8()? != GENERATION_CURRENT { return Err(WireError::UnsupportedVersion); }
    let flags = reader.u8()?;
    if flags & !ALLOWED_FLAGS != 0 { return Err(WireError::InvalidFlags); }
    Ok(flags)
}

#[derive(Clone, Copy)]
struct GlobalPrefix {
    version: u16,
    input_count: u32,
    output_count: u8,
    locktime: u64,
    subnetwork_id: [u8; 20],
    gas: u64,
    payload_len: usize,
}

fn read_global_prefix(reader: &mut Reader<'_>, limits: DecodeLimits) -> Result<GlobalPrefix, WireError> {
    let (version, input_count, output_count) = read_global_counts(reader, limits)?;
    let (locktime, subnetwork_id, gas, payload_len) = read_global_tail(reader, limits)?;
    Ok(GlobalPrefix {
        version,
        input_count,
        output_count,
        locktime,
        subnetwork_id,
        gas,
        payload_len,
    })
}

fn read_global_counts(
    reader: &mut Reader<'_>,
    limits: DecodeLimits,
) -> Result<(u16, u32, u8), WireError> {
    let version = reader.u16()?;
    let input_count = reader.u32()?;
    limits.validate_inputs(input_count)?;
    let output_count = reader.u8()?;
    limits.validate_outputs(output_count)?;
    Ok((version, input_count, output_count))
}

fn read_global_tail(
    reader: &mut Reader<'_>,
    limits: DecodeLimits,
) -> Result<(u64, [u8; 20], u64, usize), WireError> {
    let locktime = reader.u64()?;
    let subnetwork_id = reader.array::<20>()?;
    let gas = reader.u64()?;
    let payload_len = usize::from(reader.u16()?);
    limits.validate_payload(payload_len)?;
    Ok((locktime, subnetwork_id, gas, payload_len))
}

fn read_global<'a>(
    flags: u8,
    prefix: GlobalPrefix,
    reader: &mut Reader<'a>,
) -> Result<Global<'a>, WireError> {
    let payload = reader.bytes(prefix.payload_len)?;
    Ok(Global {
        flags,
        version: prefix.version,
        input_count: prefix.input_count,
        output_count: prefix.output_count,
        locktime: prefix.locktime,
        subnetwork_id: prefix.subnetwork_id,
        gas: prefix.gas,
        payload,
    })
}

const MIN_INPUT_WIRE_BYTES: usize = 59;

fn validate_input_capacity(count: u32, remaining: usize) -> Result<(), WireError> {
    let count = usize::try_from(count).map_err(|_| WireError::CountOverflow)?;
    let minimum = count.checked_mul(MIN_INPUT_WIRE_BYTES).ok_or(WireError::CountOverflow)?;
    if minimum > remaining { return Err(WireError::CountOverflow); }
    Ok(())
}

fn read_inputs<S: DecodeSink>(reader: &mut Reader<'_>, count: u32, sink: &mut S) -> Result<(), DecodeError<S::Error>> {
    for index in 0..count { read_input(reader, index, sink)?; }
    Ok(())
}

fn read_input<S: DecodeSink>(reader: &mut Reader<'_>, index: u32, sink: &mut S) -> Result<(), DecodeError<S::Error>> {
    let input = read_input_value(reader)?;
    let signature_count = read_signature_count(reader)?;
    sink.input(index, input, signature_count).map_err(DecodeError::Sink)?;
    read_signatures(reader, index, signature_count, sink)?;
    read_redeem(reader, index, sink)
}

fn read_input_value<'a>(reader: &mut Reader<'a>) -> Result<Input<'a>, WireError> {
    Ok(Input {
        previous_tx_id: reader.array::<32>()?,
        previous_index: reader.u32()?,
        amount: reader.u64()?,
        sequence: reader.u64()?,
        sig_op_count: reader.u8()?,
        script_version: reader.u16()?,
        script: reader.script()?,
    })
}

fn read_signature_count(reader: &mut Reader<'_>) -> Result<u8, WireError> {
    let count = reader.u8()?;
    if usize::from(count) > super::MAX_SIGNATURE_RECORDS { return Err(WireError::TooManySignatures); }
    Ok(count)
}

fn read_signatures<S: DecodeSink>(
    reader: &mut Reader<'_>,
    index: u32,
    count: u8,
    sink: &mut S,
) -> Result<(), DecodeError<S::Error>> {
    let mut seen = [false; 256];
    for slot in 0..count {
        let signature = read_signature(reader, &mut seen)?;
        sink.signature(index, slot, signature).map_err(DecodeError::Sink)?;
    }
    Ok(())
}

fn read_signature(reader: &mut Reader<'_>, seen: &mut [bool; 256]) -> Result<Signature, WireError> {
    let position = reader.u8()?;
    if seen[usize::from(position)] { return Err(WireError::DuplicateSignaturePosition); }
    seen[usize::from(position)] = true;
    let sighash = reader.u8()?;
    if !valid_sighash(sighash) { return Err(WireError::InvalidSigHashType); }
    Ok(Signature { position, sighash, bytes: reader.array::<64>()? })
}

fn read_redeem<S: DecodeSink>(
    reader: &mut Reader<'_>,
    index: u32,
    sink: &mut S,
) -> Result<(), DecodeError<S::Error>> {
    let redeem_len = usize::from(reader.u16()?);
    let redeem = reader.bytes(redeem_len)?;
    sink.redeem(index, redeem).map_err(DecodeError::Sink)
}

fn read_outputs<S: DecodeSink>(reader: &mut Reader<'_>, count: u8, sink: &mut S) -> Result<(), DecodeError<S::Error>> {
    for index in 0..count {
        let output = Output { amount: reader.u64()?, script_version: reader.u16()?, script: reader.script()? };
        sink.output(index, output).map_err(DecodeError::Sink)?;
    }
    Ok(())
}

struct TrailerState {
    network: bool,
    stealth: bool,
    input_derivation: [bool; 256],
    output_derivation: [bool; 256],
    input_ms45: [bool; 256],
    output_ms45: [bool; 256],
    covenant: [bool; 256],
}
impl TrailerState {
    const fn new() -> Self { Self { network:false, stealth:false, input_derivation:[false;256], output_derivation:[false;256], input_ms45:[false;256], output_ms45:[false;256], covenant:[false;256] } }
}

fn read_trailers<S: DecodeSink>(reader: &mut Reader<'_>, inputs: u32, outputs: u8, sink: &mut S) -> Result<(), DecodeError<S::Error>> {
    let mut state = TrailerState::new();
    while let Some(marker) = reader.peek() { read_trailer(marker, reader, inputs, outputs, sink, &mut state)?; }
    if !state.network { return Err(WireError::MissingNetwork.into()); }
    Ok(())
}

fn read_trailer<S: DecodeSink>(marker: u8, reader: &mut Reader<'_>, inputs: u32, outputs: u8, sink: &mut S, state: &mut TrailerState) -> Result<(), DecodeError<S::Error>> {
    match marker {
        NETWORK_MARKER => read_network(reader, sink, state),
        MS45_INPUT_MARKER => read_ms45_input(reader, inputs, sink, state),
        MS45_OUTPUT_MARKER => read_ms45_output(reader, outputs, sink, state),
        STEALTH_MARKER => read_stealth(reader, sink, state),
        COVENANT_MARKER => read_covenant(reader, inputs, outputs, sink, state),
        INPUT_DERIVATION_MARKER => read_input_derivation(reader, inputs, sink, state),
        OUTPUT_DERIVATION_MARKER => read_output_derivation(reader, outputs, sink, state),
        _ => Err(WireError::TrailingData.into()),
    }
}

fn read_network<S: DecodeSink>(reader: &mut Reader<'_>, sink: &mut S, state: &mut TrailerState) -> Result<(), DecodeError<S::Error>> {
    reader.u8()?;
    let code = reader.u8()?;
    if state.network || !valid_network(code) { return Err(WireError::InvalidNetwork.into()); }
    state.network = true;
    sink.network(code).map_err(DecodeError::Sink)
}

fn read_stealth<S: DecodeSink>(reader: &mut Reader<'_>, sink: &mut S, state: &mut TrailerState) -> Result<(), DecodeError<S::Error>> {
    reader.u8()?;
    if state.stealth { return Err(WireError::InvalidTrailer.into()); }
    state.stealth = true;
    sink.stealth(reader.array::<32>()?).map_err(DecodeError::Sink)
}

fn read_derivation_body(reader: &mut Reader<'_>) -> Result<Derivation, WireError> {
    let value = Derivation { branch: reader.u8()?, index: reader.u32()? };
    if !valid_derivation(value) { return Err(WireError::InvalidTrailer); }
    Ok(value)
}

fn read_input_derivation<S: DecodeSink>(reader: &mut Reader<'_>, inputs: u32, sink: &mut S, state: &mut TrailerState) -> Result<(), DecodeError<S::Error>> {
    reader.u8()?; let position = reader.u8()?;
    if u32::from(position) >= inputs || state.input_derivation[usize::from(position)] { return Err(WireError::InvalidTrailer.into()); }
    let value = read_derivation_body(reader)?; state.input_derivation[usize::from(position)] = true;
    sink.input_derivation(position, value).map_err(DecodeError::Sink)
}

fn read_output_derivation<S: DecodeSink>(reader: &mut Reader<'_>, outputs: u8, sink: &mut S, state: &mut TrailerState) -> Result<(), DecodeError<S::Error>> {
    reader.u8()?; let position = reader.u8()?;
    if position >= outputs || state.output_derivation[usize::from(position)] { return Err(WireError::InvalidTrailer.into()); }
    let value = read_derivation_body(reader)?; state.output_derivation[usize::from(position)] = true;
    sink.output_derivation(position, value).map_err(DecodeError::Sink)
}

fn read_ms45_body(reader: &mut Reader<'_>) -> Result<Ms45Derivation, WireError> {
    let value = Ms45Derivation { cosigner: reader.u32()?, chain: reader.u32()?, index: reader.u32()? };
    if !valid_ms45(value) { return Err(WireError::InvalidTrailer); }
    Ok(value)
}

fn read_ms45_input<S: DecodeSink>(reader: &mut Reader<'_>, inputs: u32, sink: &mut S, state: &mut TrailerState) -> Result<(), DecodeError<S::Error>> {
    reader.u8()?; let position = reader.u8()?;
    if u32::from(position) >= inputs || state.input_ms45[usize::from(position)] { return Err(WireError::InvalidTrailer.into()); }
    let value = read_ms45_body(reader)?; state.input_ms45[usize::from(position)] = true;
    sink.input_ms45(position, value).map_err(DecodeError::Sink)
}

fn read_ms45_output<S: DecodeSink>(reader: &mut Reader<'_>, outputs: u8, sink: &mut S, state: &mut TrailerState) -> Result<(), DecodeError<S::Error>> {
    reader.u8()?; let position = reader.u8()?;
    if position >= outputs || state.output_ms45[usize::from(position)] { return Err(WireError::InvalidTrailer.into()); }
    let value = read_ms45_body(reader)?; state.output_ms45[usize::from(position)] = true;
    sink.output_ms45(position, value).map_err(DecodeError::Sink)
}

fn read_covenant<S: DecodeSink>(reader: &mut Reader<'_>, inputs: u32, outputs: u8, sink: &mut S, state: &mut TrailerState) -> Result<(), DecodeError<S::Error>> {
    reader.u8()?; let position = reader.u8()?; let authorizing = reader.u16()?;
    if position >= outputs || u32::from(authorizing) >= inputs || state.covenant[usize::from(position)] { return Err(WireError::InvalidTrailer.into()); }
    let id = reader.array::<32>()?; state.covenant[usize::from(position)] = true;
    sink.covenant(position, authorizing, id).map_err(DecodeError::Sink)
}
