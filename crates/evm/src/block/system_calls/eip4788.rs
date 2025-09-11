//! [EIP-4788](https://eips.ethereum.org/EIPS/eip-4788) system call implementation.

use core::f32::consts::E;

use crate::{
    block::{BlockExecutionError, BlockValidationError},
    Evm,
};
use alloc::{boxed::Box, string::ToString};
use alloy_eips::eip4788::BEACON_ROOTS_ADDRESS;
use alloy_hardforks::EthereumHardforks;
use alloy_primitives::B256;
use op_revm::OpHaltReason;
use revm::{bytecode::eof::printer::print, context_interface::result::ResultAndState};

/// Applies the pre-block call to the [EIP-4788] beacon block root contract, using the given block,
/// chain spec, EVM.
///
/// Note: this does not commit the state changes to the database, it only transact the call.
///
/// Returns `None` if Cancun is not active or the block is the genesis block, otherwise returns the
/// result of the call.
///
/// [EIP-4788]: https://eips.ethereum.org/EIPS/eip-4788
#[inline]
pub(crate) fn transact_beacon_root_contract_call<Halt>(
    spec: impl EthereumHardforks,
    parent_beacon_block_root: Option<B256>,
    evm: &mut impl Evm<HaltReason = Halt>,
) -> Result<Option<ResultAndState<Halt>>, BlockExecutionError> {
    tracing::info!("transact_beacon_root_contract_call: {} {} {}", spec.is_cancun_active_at_timestamp(evm.block().timestamp), evm.block().timestamp, evm.block().number);
    if !spec.is_cancun_active_at_timestamp(evm.block().timestamp) {
        return Ok(None);
    }

    let parent_beacon_block_root =
        parent_beacon_block_root.ok_or(BlockValidationError::MissingParentBeaconBlockRoot)?;

    // if the block number is zero (genesis block) then the parent beacon block root must
    // be 0x0 and no system transaction may occur as per EIP-4788
    if evm.block().number == 0 {
        if !parent_beacon_block_root.is_zero() {
            return Err(BlockValidationError::CancunGenesisParentBeaconBlockRootNotZero {
                parent_beacon_block_root,
            }
            .into());
        }
        return Ok(None);
    }
    use alloy_primitives::hex;
    tracing::info!("transact_system_call: {} 0x{}", BEACON_ROOTS_ADDRESS, hex::encode(parent_beacon_block_root.0));
    let res: ResultAndState<Halt> = match evm.transact_system_call(
        alloy_eips::eip4788::SYSTEM_ADDRESS,
        BEACON_ROOTS_ADDRESS,
        parent_beacon_block_root.0.into(),
    ) {
        Ok(res) => {
            tracing::info!("transact_beacon_root_contract_call result ok, success: {:?}", res.result.is_success());
            // 分别打印 result 和 state
            use crate::block::ExecutionResult;
            match &res.result {
                ExecutionResult::Success { reason, gas_used, gas_refunded, logs, output } => {
                    tracing::info!("res.result.Success: {:?} {:?} {:?} {:?} {:?}", reason, gas_used, gas_refunded, logs, output);
                }
                ExecutionResult::Revert { gas_used, output } => {
                    tracing::info!("res.result.Revert: {} {}", gas_used, output);
                }
                ExecutionResult::Halt { reason, gas_used, .. } => {
                    use op_revm::OpHaltReason;
                    let op_reason = unsafe { std::mem::transmute::<&Halt, &OpHaltReason>(reason) };
                    tracing::info!("res.result.Halt: {:?} {:?}", gas_used, op_reason);
                }
            }
            tracing::info!("res.state (EvmState) 包含 {} 个账户:", res.state.len());
            
            // 详细打印每个账户信息
            for (address, account) in &res.state {
                tracing::info!("  地址: {:?}", address);
                tracing::info!("  账户信息: {:?}", account);
            }
            res
        },
        Err(e) => {
            tracing::info!("transact_system_call result err: {:?}", e);
            return Err(BlockValidationError::BeaconRootContractCall {
                parent_beacon_block_root: Box::new(parent_beacon_block_root),
                message: e.to_string(),
            }
            .into())
        }
    };

    Ok(Some(res))
}
