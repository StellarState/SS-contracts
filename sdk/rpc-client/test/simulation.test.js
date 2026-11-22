import assert from "node:assert/strict";
import test from "node:test";
import {
  Account,
  Networks,
  Operation,
  SorobanDataBuilder,
  TransactionBuilder,
} from "@stellar/stellar-sdk";
import { estimateTransactionResources } from "../dist/index.js";

test("returns simulated fees and host resource estimates", async () => {
  const transactionData = new SorobanDataBuilder().setResources(1234, 2048, 512);
  const server = {
    simulateTransaction: async () => ({
      _parsed: true,
      id: "1",
      latestLedger: 42,
      events: [],
      transactionData,
      minResourceFee: "987",
    }),
  };
  const estimate = await estimateTransactionResources(server);
  assert.equal(estimate.kind, "estimate");
  assert.equal(estimate.minResourceFee, "987");
  assert.equal(estimate.latestLedger, 42);
  assert.equal(estimate.instructions, 1234);
  assert.equal(estimate.diskReadBytes, 2048);
  assert.equal(estimate.writeBytes, 512);
  assert.equal(estimate.readOnlyLedgerKeys, 0);
  assert.equal(estimate.readWriteLedgerKeys, 0);
});

test("reports simulation errors to the caller", async () => {
  const server = {
    simulateTransaction: async () => ({
      _parsed: true,
      id: "1",
      latestLedger: 42,
      events: [],
      error: "contract trap",
    }),
  };
  await assert.rejects(
    estimateTransactionResources(server),
    /Soroban simulation failed: contract trap/,
  );
});

test("returns an unsigned restoration transaction and can re-simulate", async () => {
  const account = new Account(
    "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    "7",
  );
  const transaction = new TransactionBuilder(account, {
    fee: "100",
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(Operation.restoreFootprint({}))
    .setTimeout(30)
    .build();
  const preambleData = new SorobanDataBuilder().setResources(10, 20, 30);
  const simulation = {
    _parsed: true,
    id: "1",
    latestLedger: 42,
    events: [],
    transactionData: new SorobanDataBuilder().setResources(1234, 2048, 512),
    minResourceFee: "987",
    result: { auth: [] },
    restorePreamble: {
      minResourceFee: "654",
      transactionData: preambleData,
    },
  };
  let calls = 0;
  const server = {
    simulateTransaction: async () => {
      calls += 1;
      return calls === 1
        ? simulation
        : {
            _parsed: true,
            id: "2",
            latestLedger: 43,
            events: [],
            transactionData: new SorobanDataBuilder().setResources(1, 2, 3),
            minResourceFee: "321",
          };
    },
  };

  const outcome = await estimateTransactionResources(server, transaction, {
    networkPassphrase: Networks.TESTNET,
  });
  assert.equal(outcome.kind, "restoration_required");
  assert.equal(outcome.restorePreamble.minResourceFee, "654");
  assert.equal(outcome.restorationTransaction.operations[0].type, "restoreFootprint");
  assert.deepEqual(
    outcome.restorationTransaction.toEnvelope().v1.tx.ext.sorobanData.resources,
    preambleData.build().resources,
  );

  const estimate = await outcome.resimulate();
  assert.equal(estimate.kind, "estimate");
  assert.equal(estimate.minResourceFee, "321");
  assert.equal(calls, 2);
});
