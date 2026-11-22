import {
  Account,
  Contract,
  Keypair,
  Networks,
  Operation,
  rpc,
  TransactionBuilder,
  xdr,
} from "@stellar/stellar-sdk";

export interface RpcClientOptions {
  contractId: string;
  rpcUrl: string;
  networkPassphrase?: string;
  timeoutMs?: number;
  /** Maximum number of getTransaction calls after a submission. */
  pollAttempts?: number;
  /** Number of replacement submissions after a temporary rejection. */
  submissionRetries?: number;
  pollIntervalMs?: number;
  retryDelayMs?: number;
}

export interface ConfirmedSubmission {
  status: "confirmed";
  hash: string;
  ledger: number;
  result: rpc.Api.GetSuccessfulTransactionResponse;
}

export interface AcceptedSubmission {
  status: "accepted";
  hash: string;
  sendStatus: "PENDING" | "DUPLICATE";
}

export interface FailedSubmission {
  status: "failed";
  hash: string;
  result?: rpc.Api.GetFailedTransactionResponse;
  errorResult?: xdr.TransactionResult;
  diagnosticEvents?: xdr.DiagnosticEvent[];
}

export interface UnresolvedSubmission {
  status: "unresolved";
  hash: string;
  lastKnownStatus: rpc.Api.GetTransactionResponse;
  attempts: number;
}

export type SubmissionResult =
  | AcceptedSubmission
  | ConfirmedSubmission
  | FailedSubmission
  | UnresolvedSubmission;

export class RpcSubmissionError extends Error {
  constructor(message: string, readonly cause?: unknown) {
    super(message);
    this.name = "RpcSubmissionError";
  }
}

/** Small convenience wrapper for read and write calls to InvoiceEscrow. */
export class InvoiceEscrowRpcClient {
  readonly server: rpc.Server;
  readonly contractId: string;
  readonly networkPassphrase: string;
  readonly pollAttempts: number;
  readonly submissionRetries: number;
  readonly pollIntervalMs: number;
  readonly retryDelayMs: number;

  constructor(options: RpcClientOptions) {
    this.contractId = options.contractId;
    this.networkPassphrase = options.networkPassphrase ?? Networks.TESTNET;
    this.server = new rpc.Server(options.rpcUrl, {
      allowHttp: options.rpcUrl.startsWith("http://"),
      timeout: options.timeoutMs,
    });
    this.pollAttempts = options.pollAttempts ?? 5;
    this.submissionRetries = options.submissionRetries ?? 2;
    this.pollIntervalMs = options.pollIntervalMs ?? 1_000;
    this.retryDelayMs = options.retryDelayMs ?? 250;
  }

  async query<T>(method: string, args: Record<string, unknown> = {}): Promise<T> {
    const { result } = await this.server.queryContract<T>(
      this.contractId,
      method,
      args,
      this.networkPassphrase,
    );
    return result;
  }

  getEscrow<T = unknown>(invoiceId: string): Promise<T> {
    return this.query<T>("get_escrow", { invoice_id: invoiceId });
  }

  getEscrowStatus<T = unknown>(invoiceId: string): Promise<T> {
    return this.query<T>("get_escrow_status", { invoice_id: invoiceId });
  }

  getConfig<T = unknown>(): Promise<T> {
    return this.query<T>("get_config");
  }

  /** Submit a contract method using its already encoded Soroban arguments. */
  async submit(method: string, args: xdr.ScVal[], signer: Keypair): Promise<SubmissionResult> {
    let lastHash: string | undefined;
    let lastKnownStatus: rpc.Api.GetTransactionResponse | undefined;

    for (let attempt = 0; attempt <= this.submissionRetries; attempt += 1) {
      if (lastHash) {
        const recovered = await this.server.getTransaction(lastHash);
        lastKnownStatus = recovered;
        const recoveredResult = this.toSubmissionResult(recovered, lastHash);
        if (recoveredResult) return recoveredResult;
      }

      const sent = await this.send(method, args, signer);
      lastHash = sent.hash;
      if (sent.status === "ERROR") {
        return {
          status: "failed",
          hash: sent.hash,
          errorResult: sent.errorResult,
          diagnosticEvents: sent.diagnosticEvents,
        };
      }
      if (this.pollAttempts === 0 && (sent.status === "PENDING" || sent.status === "DUPLICATE")) {
        return { status: "accepted", hash: sent.hash, sendStatus: sent.status };
      }

      const confirmed = await this.confirm(sent.hash);
      if (confirmed.status !== "unresolved") return confirmed;
      lastKnownStatus = confirmed.lastKnownStatus;

      if (sent.status !== "TRY_AGAIN_LATER" || attempt === this.submissionRetries) {
        return confirmed;
      }
      await this.delay(this.retryDelayMs);
    }

    // The loop always returns, but retaining this guard makes the contract explicit
    // if the retry policy is changed later.
    throw new RpcSubmissionError(
      `RPC submission ended without a result${lastHash ? ` for ${lastHash}` : ""}`,
      lastKnownStatus,
    );
  }

  private async send(method: string, args: xdr.ScVal[], signer: Keypair) {
    const account: Account = await this.server.getAccount(signer.publicKey());
    const tx = new TransactionBuilder(account, {
      fee: "100",
      networkPassphrase: this.networkPassphrase,
    })
      .addOperation(new Contract(this.contractId).call(method, ...args))
      .setTimeout(30)
      .build();
    const prepared = await this.server.prepareTransaction(tx);
    prepared.sign(signer);
    return this.server.sendTransaction(prepared);
  }

  private async confirm(hash: string): Promise<SubmissionResult> {
    let lastKnownStatus: rpc.Api.GetTransactionResponse = {
      status: rpc.Api.GetTransactionStatus.NOT_FOUND,
      txHash: hash,
      latestLedger: 0,
      latestLedgerCloseTime: 0,
      oldestLedger: 0,
      oldestLedgerCloseTime: 0,
    };
    for (let attempt = 1; attempt <= this.pollAttempts; attempt += 1) {
      lastKnownStatus = await this.server.getTransaction(hash);
      const result = this.toSubmissionResult(lastKnownStatus, hash);
      if (result) return result;
      if (attempt < this.pollAttempts) await this.delay(this.pollIntervalMs);
    }
    return { status: "unresolved", hash, lastKnownStatus, attempts: this.pollAttempts };
  }

  private toSubmissionResult(
    result: rpc.Api.GetTransactionResponse,
    hash: string,
  ): SubmissionResult | undefined {
    if (result.status === rpc.Api.GetTransactionStatus.SUCCESS) {
      return { status: "confirmed", hash, ledger: result.ledger, result };
    }
    if (result.status === rpc.Api.GetTransactionStatus.FAILED) {
      return { status: "failed", hash, result };
    }
    return undefined;
  }

  private delay(milliseconds: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, milliseconds));
  }

  /** Exposes a prepared Operation for clients that need custom transaction composition. */
  contractCall(method: string, ...args: xdr.ScVal[]): Operation {
    return new Contract(this.contractId).call(method, ...args);
  }
}
