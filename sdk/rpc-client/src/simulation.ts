import {
  Account,
  Networks,
  Operation,
  rpc,
  Transaction,
  TransactionBuilder,
  xdr,
} from "@stellar/stellar-sdk";

export interface ResourceEstimate {
  kind: "estimate";
  minResourceFee: string;
  latestLedger: number;
  instructions: number;
  diskReadBytes: number;
  writeBytes: number;
  readOnlyLedgerKeys: number;
  readWriteLedgerKeys: number;
  footprint: xdr.LedgerFootprint;
  simulation: rpc.Api.SimulateTransactionSuccessResponse;
}

export interface RestorationRequired {
  kind: "restoration_required";
  restorePreamble: rpc.Api.SimulateTransactionRestoreResponse["restorePreamble"];
  /** An unsigned transaction that restores the footprint from restorePreamble. */
  restorationTransaction: Transaction;
  simulation: rpc.Api.SimulateTransactionRestoreResponse;
  /** Re-simulate the original invocation after the restoration is confirmed. */
  resimulate: () => Promise<SimulationOutcome>;
}

export type SimulationOutcome = ResourceEstimate | RestorationRequired;

export interface EstimateTransactionResourcesOptions {
  /** Network used when constructing a possible unsigned restoration transaction. */
  networkPassphrase?: string;
}

function buildRestorationTransaction(
  transaction: Transaction,
  restorePreamble: RestorationRequired["restorePreamble"],
  networkPassphrase: string,
): Transaction {
  const source = new Account(transaction.source, transaction.sequence);
  return new TransactionBuilder(source, {
    fee: restorePreamble.minResourceFee,
    networkPassphrase,
  })
    .setSorobanData(restorePreamble.transactionData.build())
    .addOperation(Operation.restoreFootprint({}))
    .setTimeout(30)
    .build();
}

/** Simulate a transaction before signing and return its resource and fee estimate. */
export async function estimateTransactionResources(
  server: rpc.Server,
  transaction: Transaction,
  options: EstimateTransactionResourcesOptions = {},
): Promise<SimulationOutcome> {
  const simulation = await server.simulateTransaction(transaction);
  if (rpc.Api.isSimulationError(simulation)) {
    throw new Error(`Soroban simulation failed: ${simulation.error}`);
  }
  if (rpc.Api.isSimulationRestore(simulation)) {
    const networkPassphrase = options.networkPassphrase ?? Networks.TESTNET;
    return {
      kind: "restoration_required",
      restorePreamble: simulation.restorePreamble,
      restorationTransaction: buildRestorationTransaction(
        transaction,
        simulation.restorePreamble,
        networkPassphrase,
      ),
      simulation,
      resimulate: () => estimateTransactionResources(server, transaction, options),
    };
  }
  const footprint = simulation.transactionData.getFootprint();
  const resources = simulation.transactionData.build().resources;
  return {
    kind: "estimate",
    minResourceFee: simulation.minResourceFee,
    latestLedger: simulation.latestLedger,
    instructions: resources.instructions,
    diskReadBytes: resources.diskReadBytes,
    writeBytes: resources.writeBytes,
    readOnlyLedgerKeys: footprint.readOnly.length,
    readWriteLedgerKeys: footprint.readWrite.length,
    footprint,
    simulation,
  };
}

/** Build an unsigned invocation and estimate its Soroban resource fee. */
export async function simulateContractCall(
  server: rpc.Server,
  source: Account,
  operation: xdr.Operation,
  networkPassphrase: string,
): Promise<SimulationOutcome> {
  const tx = new TransactionBuilder(source, {
    fee: "100",
    networkPassphrase,
  })
    .addOperation(operation)
    .setTimeout(30)
    .build();
  return estimateTransactionResources(server, tx, { networkPassphrase });
}
