//! Main payments engine processing.

use std::io::{BufReader, BufWriter, Read, Write};

use anyhow::Result;
use csv::Trim;

use crate::types::{Account, Accounts, CachedTx, ClientId, Transaction, TransactionType, TxCache};

/// Run
///
/// Read and process all transactions from `input` (trait bound `std::io::Read`) and write
/// the results to `output` (trait bound `std::io::Write`).
pub fn run<R: Read, W: Write>(input: &mut R, output: &mut W) -> Result<()> {
	let mut accounts = Accounts::new();
	let mut tx_cache = TxCache::new();
	process_transactions(input, &mut accounts, &mut tx_cache)?;
	write_accounts(output, accounts)
}

/// Process Transactions
///
/// Process all transactions from a reader which provides transactions in a CSV format, adding
/// relevant data to client accounts (`accounts`) and using a transaction cache (`tx_cache`) to
/// cache transactions relevant for disputes.
fn process_transactions<R: Read>(
	input: &mut R,
	accounts: &mut Accounts,
	tx_cache: &mut TxCache,
) -> Result<()> {
	let buffered = BufReader::new(input);
	let mut rdr = csv::ReaderBuilder::new()
		.trim(Trim::All)
		.from_reader(buffered);

	for result in rdr.deserialize() {
		let transaction: Transaction = result?;
		log::debug!("{:?}", transaction);

		// NB this results in getting the account twice in some cases. Not the most
		// efficient, but it's easily optimized:
		if let Some(account) = accounts.get(&transaction.client) {
			if account.locked {
				continue;
			}
		}
		match transaction.r#type {
			TransactionType::Deposit => handle_deposit(accounts, tx_cache, transaction),
			TransactionType::Withdrawal => handle_withdrawal(accounts, transaction),
			TransactionType::Dispute => handle_dispute(accounts, tx_cache, transaction),
			TransactionType::Resolve => handle_resolve(accounts, tx_cache, transaction),
			TransactionType::Chargeback => handle_chargeback(accounts, tx_cache, transaction),
		}
	}

	Ok(())
}

/// Handle Deposit
///
/// If no client account exists yet, we create one (NB this is the only occasion where we create
/// new client accounts). We then increase the total funds by the transaction amount, implicitly
/// increasing the available amount and insert the new transaction into the transaction cache. If
/// the transaction ID already exists we ignore it.
fn handle_deposit(accounts: &mut Accounts, tx_cache: &mut TxCache, transaction: Transaction) {
	if tx_cache.contains_key(&transaction.tx) {
		// Transactions are globally unique, but the spec didn't say we can rely on not being passed
		// the same transaction twice.
		return;
	}
	match accounts.get_mut(&transaction.client) {
		Some(account) => account.total += transaction.amount(),
		None => {
			log::debug!("New client '{}'", transaction.client);
			accounts.insert(
				transaction.client,
				Account::new_deposit(transaction.amount()),
			);
		}
	}
	tx_cache.insert(
		transaction.tx,
		CachedTx::new(transaction.amount(), transaction.client),
	);
}

/// Handle Withdrawal
///
/// If the client account doesn't exist, we ignore this request. Otherwise remove the funds from
/// the account if the requested amount is less than the available funds. NB it's implied by the
/// spec that withdrawals are not to be disputed. Therefore we do not enter this transaction into
/// the transaction cache.
fn handle_withdrawal(accounts: &mut Accounts, transaction: Transaction) {
	match accounts.get_mut(&transaction.client) {
		Some(account) => {
			if account.available() >= transaction.amount() {
				account.total -= transaction.amount()
			} else {
				log::debug!("Ignoring withdrawal exceeding available funds");
			}
		}
		None => log::debug!("Ignoring missing client '{}'", transaction.client),
	}
}

/// Handle Dispute
///
/// If the transaction or client account doesn't exist, or the transaction is already disputed,
/// we ignore this request. Otherwise mark the transaction as disputed and the corresponding
/// funds as `held` in the client account.
fn handle_dispute(accounts: &mut Accounts, tx_cache: &mut TxCache, transaction: Transaction) {
	if let Some((account, cached_tx)) = get_existing(accounts, tx_cache, &transaction) {
		if !cached_tx.disputed {
			cached_tx.disputed = true;
			account.held += cached_tx.amount;
		} else {
			log::debug!("Ignoring already disputed tx '{}'", transaction.tx);
		}
	};
}

/// Handle Resolve
///
/// If the transaction or client account doesn't exist, or the transaction is not currently
/// disputed, we ignore this request. Otherwise mark the transaction as no longer disputed
/// and release the corresponding funds in the client account.
fn handle_resolve(accounts: &mut Accounts, tx_cache: &mut TxCache, transaction: Transaction) {
	if let Some((account, cached_tx)) = get_existing(accounts, tx_cache, &transaction) {
		if cached_tx.disputed {
			account.held -= cached_tx.amount;
			cached_tx.disputed = false;
		} else {
			log::debug!("Ignoring resolve on undiputed tx '{}'", transaction.tx);
		}
	}
}

/// Handle Chargeback
///
/// If the transaction or client account doesn't exist, or the transaction is not currently
/// disputed, we ignore this request. Otherwise mark the transaction as no longer disputed,
/// remove disputed funds from the client account, and lock the account.
fn handle_chargeback(accounts: &mut Accounts, tx_cache: &mut TxCache, transaction: Transaction) {
	if let Some((account, cached_tx)) = get_existing(accounts, tx_cache, &transaction) {
		if cached_tx.disputed {
			account.held -= cached_tx.amount;
			account.total -= cached_tx.amount;
			account.locked = true;
			cached_tx.disputed = false;
		} else {
			log::debug!("Ignoring chargeback on undiputed tx '{}'", transaction.tx);
		}
	}
}

/// Get Existing
///
/// Convenience function to get client account, get cached transaction and check that the client
/// ID matches the incoming transaction, logging any relevant information if something is awry.
/// Used in handling disputes, resolves and chargebacks.
fn get_existing<'a>(
	accounts: &'a mut Accounts,
	tx_cache: &'a mut TxCache,
	transaction: &Transaction,
) -> Option<(&'a mut Account, &'a mut CachedTx)> {
	if let Some(account) = accounts.get_mut(&transaction.client) {
		if let Some(cached_tx) = tx_cache.get_mut(&transaction.tx) {
			if cached_tx.client == transaction.client {
				return Some((account, cached_tx));
			} else {
				log::debug!("Ignoring client mismatch for tx '{}'", transaction.tx);
			}
		} else {
			log::debug!("Ignoring missing tx '{}'", transaction.tx);
		}
	} else {
		log::debug!("Ignoring missing client '{}'", transaction.client);
	};
	None
}

/// Write Accounts
///
/// Write all account information as a CSV to `wtr` (required trait bound `std::io::Write`).
fn write_accounts<W: Write>(wtr: &mut W, accounts: Accounts) -> Result<()> {
	let mut buffered = BufWriter::new(wtr);
	write!(buffered, "client,available,held,total,locked\n")?;
	for (client, account) in accounts.iter() {
		write_account(&mut buffered, client, account)?;
	}
	buffered.flush()?;
	Ok(())
}

/// Write Account
///
/// Simple function to print a client account.
fn write_account<W: Write>(wtr: &mut W, client: &ClientId, account: &Account) -> Result<()> {
	write!(
		wtr,
		"{},{:.4},{:.4},{:.4},{}\n",
		client,
		account.available(),
		account.held,
		account.total,
		account.locked
	)?;
	Ok(())
}

#[cfg(test)]
mod test {
	use super::*;
	use rstest::*;

	impl Transaction {
		/// Convenience function for tests
		fn new() -> Self {
			Transaction {
				// NB `TransactionType` doesn't actually make a difference once we enter a `handle_`
				// function.
				r#type: TransactionType::Deposit,
				client: 1,
				tx: 1,
				amount: Some(1.0),
			}
		}
	}

	#[fixture]
	fn accounts() -> Accounts {
		Accounts::new()
	}

	#[fixture]
	fn tx_cache() -> TxCache {
		TxCache::new()
	}

	#[rstest]
	fn handle_deposit_creates_account(mut accounts: Accounts, mut tx_cache: TxCache) {
		let deposit = Transaction::new();
		handle_deposit(&mut accounts, &mut tx_cache, deposit);
		assert_eq!(accounts.len(), 1);
		assert_eq!(tx_cache.len(), 1);
	}

	#[rstest]
	fn handle_deposit_adds_to_existing_account(mut accounts: Accounts, mut tx_cache: TxCache) {
		let deposit1 = Transaction::new();
		let mut deposit2 = Transaction::new();
		deposit2.tx = 2;
		handle_deposit(&mut accounts, &mut tx_cache, deposit1);
		handle_deposit(&mut accounts, &mut tx_cache, deposit2);
		assert_eq!(accounts.len(), 1);
		assert_eq!(tx_cache.len(), 2);

		let total = accounts.get(&1).map_or(-10.0, |x| x.total);
		assert_eq!(total, 2.0);
	}

	#[rstest]
	fn handle_withdrawal_subtracts_from_account(mut accounts: Accounts, mut tx_cache: TxCache) {
		let deposit = Transaction::new();
		let withdrawal = Transaction::new();
		handle_deposit(&mut accounts, &mut tx_cache, deposit);
		handle_withdrawal(&mut accounts, withdrawal);
		assert_eq!(accounts.len(), 1);
		assert_eq!(tx_cache.len(), 1);
	}

	#[rstest]
	fn cant_withdraw_more_than_available(mut accounts: Accounts, mut tx_cache: TxCache) {
		let deposit = Transaction::new();
		let mut withdrawal = Transaction::new();
		withdrawal.amount = Some(deposit.amount() * 2.0);
		handle_deposit(&mut accounts, &mut tx_cache, deposit);
		handle_withdrawal(&mut accounts, withdrawal);
		let total = accounts.get(&1).map_or(-10.0, |x| x.total);
		assert_eq!(total, 1.0);
	}

	#[rstest]
	fn handle_withdrawal_doesnt_create_unseen_account(mut accounts: Accounts) {
		let withdrawal = Transaction::new();
		handle_withdrawal(&mut accounts, withdrawal);
		assert_eq!(accounts.len(), 0);
	}

	#[rstest]
	fn handle_dispute_marks_funds_correctly(mut accounts: Accounts, mut tx_cache: TxCache) {
		let deposit = Transaction::new();
		let dispute = Transaction::new();
		handle_deposit(&mut accounts, &mut tx_cache, deposit);
		handle_dispute(&mut accounts, &mut tx_cache, dispute);

		let held = accounts.get(&1).map_or(-10.0, |x| x.held);
		assert_eq!(held, 1.0);

		let disputed = tx_cache.get(&1).map_or(false, |x| x.disputed);
		assert!(disputed);
	}

	#[rstest]
	fn handle_dispute_doesnt_effect_wrong_client(mut accounts: Accounts, mut tx_cache: TxCache) {
		let deposit = Transaction::new();
		let mut dispute = Transaction::new();
		dispute.client = 2;
		handle_deposit(&mut accounts, &mut tx_cache, deposit);
		handle_dispute(&mut accounts, &mut tx_cache, dispute);

		let held = accounts.get(&1).map_or(-10.0, |x| x.held);
		assert_eq!(held, 0.0);

		let disputed = tx_cache.get(&1).map_or(true, |x| x.disputed);
		assert!(!disputed);
	}

	#[rstest]
	fn total_funds_go_negative(mut accounts: Accounts, mut tx_cache: TxCache) {
		let deposit = Transaction::new();
		let withdrawal = Transaction::new();
		let dispute = Transaction::new();
		let chargeback = Transaction::new();
		handle_deposit(&mut accounts, &mut tx_cache, deposit);
		handle_withdrawal(&mut accounts, withdrawal);
		handle_dispute(&mut accounts, &mut tx_cache, dispute);
		handle_chargeback(&mut accounts, &mut tx_cache, chargeback);

		let total = accounts.get(&1).map_or(-10.0, |x| x.total);
		assert_eq!(total, -1.0);

		let locked = accounts.get(&1).map_or(false, |x| x.locked);
		assert!(locked);
	}
}
