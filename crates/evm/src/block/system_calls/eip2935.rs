//! [EIP-2935](https://eips.ethereum.org/EIPS/eip-2935) system call implementation.

use crate::{
    block::{BlockExecutionError, BlockValidationError},
    Evm,
};
use alloc::string::ToString;
use alloy_eips::eip2935::HISTORY_STORAGE_ADDRESS;
use alloy_hardforks::EthereumHardforks;
use alloy_primitives::B256;
use revm::{context::result::HaltReason, context_interface::result::ResultAndState};

/// Applies the pre-block call to the [EIP-2935] blockhashes contract, using the given block,
/// chain specification, and EVM.
///
/// If Prague is not activated, or the block is the genesis block, then this is a no-op, and no
/// state changes are made.
///
/// Note: this does not commit the state changes to the database, it only transact the call.
///
/// Returns `None` if Prague is not active or the block is the genesis block, otherwise returns the
/// result of the call.
///
/// [EIP-2935]: https://eips.ethereum.org/EIPS/eip-2935
#[inline]
pub(crate) fn transact_blockhashes_contract_call<Halt>(
    spec: impl EthereumHardforks,
    parent_block_hash: B256,
    evm: &mut impl Evm<HaltReason = Halt>,
) -> Result<Option<ResultAndState<Halt>>, BlockExecutionError> {
    tracing::info!("transact_blockhashes_contract_call: {} {} {}", spec.is_prague_active_at_timestamp(evm.block().timestamp), evm.block().timestamp, evm.block().number);
    if !spec.is_prague_active_at_timestamp(evm.block().timestamp) {
        return Ok(None);
    }

    // if the block number is zero (genesis block) then no system transaction may occur as per
    // EIP-2935
    if evm.block().number == 0 {
        return Ok(None);
    }

    use alloy_primitives::hex;
    tracing::info!("transact_blockhashes_contract_call: 0x{}", hex::encode(parent_block_hash.0));
    let res: ResultAndState<Halt> = match evm.transact_system_call(
        alloy_eips::eip4788::SYSTEM_ADDRESS,
        HISTORY_STORAGE_ADDRESS,
        parent_block_hash.0.into(),
    ) {
        Ok(res) => {
            tracing::info!("transact_blockhashes_contract_call result ok, success: {:?}", res.result.is_success());
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
            return Err(
                BlockValidationError::BlockHashContractCall { message: e.to_string() }.into()
            )
        }
    };

    Ok(Some(res))
}
