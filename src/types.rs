//! Global type definitions.

use std::collections::HashMap;

use serde::Deserialize;

pub type Amount = f64;
pub type ClientId = u16;
pub type TransactionId = u32;

/// Maps client IDs to their current output state.
pub type Accounts = HashMap<ClientId, Account>;

/// Cache of transactions for handling disputes.
pub type TxCache = HashMap<TransactionId, CachedTx>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
	Deposit,
	Withdrawal,
	Dispute,
	Resolve,
	Chargeback,
}

#[derive(Debug)]
pub struct Account {
	// NB `available` is an inferred value
	pub held: Amount,
	pub total: Amount,
	pub locked: bool,
}

#[derive(Debug)]
pub struct CachedTx {
	pub amount: Amount,
	pub client: ClientId,
	pub disputed: bool,
}

#[derive(Debug, Deserialize)]
pub struct Transaction {
	pub r#type: TransactionType,
	pub client: ClientId,
	pub tx: TransactionId,
	pub amount: Option<Amount>,
}

impl Account {
	pub fn new_deposit(amount: Amount) -> Self {
		Self {
			held: 0.0,
			total: amount,
			locked: false,
		}
	}

	pub fn available(&self) -> Amount {
		self.total - self.held
	}
}

impl Transaction {
	/// For simplicty, we return a default amount of 0.0 if amount is missing, thereby avoiding
	/// handling `Option`s in various handlers.
	pub fn amount(&self) -> Amount {
		match self.amount {
			Some(x) => x,
			None => 0.0,
		}
	}
}

impl CachedTx {
	pub fn new(amount: Amount, client: ClientId) -> Self {
		Self {
			amount,
			client,
			disputed: false,
		}
	}
}
