import { HexString } from '@gear-js/api';
import { useMutation } from '@tanstack/react-query';
import { encodeFunctionData } from 'viem';
import { useConfig, useWriteContract } from 'wagmi';
import { estimateFeesPerGas, estimateGas, watchContractEvent } from 'wagmi/actions';

import { ETH_WRAPPED_ETH_CONTRACT_ADDRESS } from '@/consts/env';
import { isUndefined } from '@/utils';

import { BRIDGING_PAYMENT_ABI, ETH_BRIDGING_PAYMENT_CONTRACT_ADDRESS } from '../../consts';
import { InsufficientAccountBalanceError } from '../../errors';
import { FormattedValues } from '../../types';

import { useApprove } from './use-approve';
import { useMint } from './use-mint';

const TRANSFER_GAS_LIMIT_FALLBACK = 21000n * 10n;

function useHandleEthSubmit(
  ftAddress: HexString | undefined,
  fee: bigint | undefined,
  allowance: bigint | undefined,
  ftBalance: bigint | undefined,
  accountBalance: bigint | undefined,
  openTransactionModal: (amount: string, receiver: string) => void,
) {
  const { writeContractAsync } = useWriteContract();
  const mint = useMint(ftAddress);
  const approve = useApprove(ftAddress);
  const config = useConfig();

  const getTransferGasLimit = (amount: bigint, accountAddress: HexString) => {
    if (!ftAddress) throw new Error('Fungible token address is not defined');

    const encodedData = encodeFunctionData({
      abi: BRIDGING_PAYMENT_ABI,
      functionName: 'requestBridging',
      args: [ftAddress, amount, accountAddress],
    });

    return estimateGas(config, {
      to: ETH_BRIDGING_PAYMENT_CONTRACT_ADDRESS,
      data: encodedData,
      value: fee,
    });
  };

  const validateBalance = async (amount: bigint, accountAddress: HexString) => {
    if (!ftAddress) throw new Error('Fungible token address is not defined');
    if (isUndefined(fee)) throw new Error('Fee is not defined');
    if (isUndefined(allowance)) throw new Error('Allowance is not defined');
    if (isUndefined(ftBalance)) throw new Error('FT balance is not found');
    if (isUndefined(accountBalance)) throw new Error('Account balance is not defined');

    const isMintRequired = ftAddress === ETH_WRAPPED_ETH_CONTRACT_ADDRESS && amount > ftBalance;
    console.log('isMintRequired: ', isMintRequired);
    const valueToMint = isMintRequired ? amount - ftBalance : BigInt(0);
    console.log('valueToMint: ', valueToMint);
    const mintGasLimit = isMintRequired ? await mint.getGasLimit(valueToMint) : BigInt(0);

    const isApproveRequired = amount > allowance;
    const approveGasLimit = isApproveRequired ? await approve.getGasLimit(amount) : BigInt(0);

    // if approve is not made, transfer gas estimate will fail.
    // it can be avoided by using stateOverride,
    // but it requires the knowledge of the storage slot or state diff of the allowance for each token,
    // which is not feasible to do programmatically (at least I didn't managed to find a convenient way to do so).
    const transferGasLimit = isApproveRequired ? undefined : await getTransferGasLimit(amount, accountAddress);

    // TRANSFER_GAS_LIMIT_FALLBACK is just for balance check, during the actual transfer it will be recalculated
    const gasLimit = mintGasLimit + approveGasLimit + (transferGasLimit || TRANSFER_GAS_LIMIT_FALLBACK);

    const { maxFeePerGas } = await estimateFeesPerGas(config);
    const weiGasLimit = gasLimit * maxFeePerGas;

    const balanceToWithdraw = valueToMint + weiGasLimit + fee;

    if (balanceToWithdraw > accountBalance) throw new InsufficientAccountBalanceError('ETH', balanceToWithdraw);

    return { valueToMint, isMintRequired, isApproveRequired, mintGasLimit, approveGasLimit, transferGasLimit };
  };

  const transfer = async (amount: bigint, accountAddress: HexString, gasLimit: bigint | undefined) => {
    if (!ftAddress) throw new Error('Fungible token address is not defined');
    if (!fee) throw new Error('Fee is not defined');

    return writeContractAsync({
      abi: BRIDGING_PAYMENT_ABI,
      address: ETH_BRIDGING_PAYMENT_CONTRACT_ADDRESS,
      functionName: 'requestBridging',
      args: [ftAddress, amount, accountAddress],
      value: fee,
      gas: gasLimit,
    });
  };

  const watch = () =>
    new Promise<void>((resolve, reject) => {
      const onError = (error: Error) => {
        unwatch();
        reject(error);
      };

      const onLogs = () => {
        unwatch();
        resolve();
      };

      const address = ETH_BRIDGING_PAYMENT_CONTRACT_ADDRESS;
      const abi = BRIDGING_PAYMENT_ABI;

      const unwatch = watchContractEvent(config, { address, abi, eventName: 'FeePaid', onLogs, onError });
    });

  const onSubmit = async ({ amount, accountAddress }: FormattedValues) => {
    const { valueToMint, isMintRequired, isApproveRequired, mintGasLimit, approveGasLimit, transferGasLimit } =
      await validateBalance(amount, accountAddress);

    openTransactionModal(amount.toString(), accountAddress);

    if (isMintRequired) {
      await mint.mutateAsync({ value: valueToMint, gas: mintGasLimit });
    } else {
      mint.reset();
    }

    if (isApproveRequired) {
      await approve.mutateAsync({ amount, gas: approveGasLimit });
    } else {
      approve.reset();
    }

    return transfer(amount, accountAddress, transferGasLimit).then(() => watch());
  };

  const submit = useMutation({ mutationFn: onSubmit });

  return [submit, approve, undefined, mint] as const;
}

export { useHandleEthSubmit };
