const ERC20_MANAGER_ABI = [
  {
    type: 'event',
    name: 'BridgingAccepted',
    inputs: [
      { name: 'to', type: 'address', indexed: true, internalType: 'address' },
      { name: 'token', type: 'address', indexed: true, internalType: 'address' },
      { name: 'amount', type: 'uint256', indexed: false, internalType: 'uint256' },
    ],
    anonymous: false,
  },
  {
    type: 'event',
    name: 'BridgingRequested',
    inputs: [
      { name: 'from', type: 'address', indexed: true, internalType: 'address' },
      { name: 'to', type: 'bytes32', indexed: true, internalType: 'bytes32' },
      { name: 'token', type: 'address', indexed: true, internalType: 'address' },
      { name: 'amount', type: 'uint256', indexed: false, internalType: 'uint256' },
    ],
    anonymous: false,
  },
  { type: 'error', name: 'BadArguments', inputs: [] },
  { type: 'error', name: 'BadVftManagerAddress', inputs: [] },
  { type: 'error', name: 'NotAuthorized', inputs: [] },
  { type: 'error', name: 'UnsupportedTokenSupply', inputs: [] },
] as const;

export { ERC20_MANAGER_ABI };
