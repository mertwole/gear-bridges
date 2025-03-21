import { createContext, PropsWithChildren, useContext, useMemo, useState } from 'react';

type BridgeContextType = {
  pairIndex: number;
  setPairIndex: (index: number) => void;
};

const DEFAULT_BRIDGE_CONTEXT: BridgeContextType = {
  pairIndex: 0,
  setPairIndex: () => {},
} as const;

const BridgeContext = createContext(DEFAULT_BRIDGE_CONTEXT);
const { Provider } = BridgeContext;
const useBridge = () => useContext(BridgeContext);

function BridgeProvider({ children }: PropsWithChildren) {
  const [pairIndex, setPairIndex] = useState(0);

  const value = useMemo(() => ({ pairIndex, setPairIndex }), [pairIndex]);

  return <Provider value={value}>{children}</Provider>;
}

// eslint-disable-next-line react-refresh/only-export-components -- TODO: figure out
export { BridgeProvider, useBridge };
