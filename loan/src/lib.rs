//! Loan registry. Only the platform (or the owner) can create a loan or change its status.
//!
//! HIRING TASK (10–15 min): implement `record_payment` + `remaining_of`, and fix `mark_defaulted`.
//! Instructions are in the repo README under "Your task". Do not edit the tests.

use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Address(pub [u8; 20]);

impl Address {
    pub const fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0u8; 20])
    }

    pub fn is_zero(self) -> bool {
        self == Self::zero()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoanStatus {
    Pending,
    Approved,
    Disbursed,
    Repaid,
    Defaulted,
    Rejected,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Loan {
    pub off_chain_id: [u8; 32],
    pub borrower: Address,
    pub amount_cents: u64,
    pub interest_rate_bps: u64,
    pub term_months: u64,
    pub monthly_payment_cents: u64,
    pub status: LoanStatus,
    pub created_at: u64,
    pub approved_at: u64,
    pub disbursed_at: u64,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Event {
    OwnershipTransferred {
        previous_owner: Address,
        new_owner: Address,
    },
    PlatformUpdated {
        previous_platform: Address,
        new_platform: Address,
    },
    LoanCreated {
        loan_id: u64,
        off_chain_id: [u8; 32],
        borrower: Address,
        amount_cents: u64,
        interest_rate_bps: u64,
        term_months: u64,
    },
    LoanStatusUpdated {
        loan_id: u64,
        previous_status: LoanStatus,
        new_status: LoanStatus,
    },
    PaymentRecorded {
        loan_id: u64,
        payer: Address,
        amount_cents: u64,
        remaining_cents: u64,
    },
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LoanError {
    Unauthorized,
    InvalidAddress,
    InvalidAmount,
    LoanExists,
    LoanNotFound,
    InvalidStatusTransition {
        from: LoanStatus,
        to: LoanStatus,
    },
}

#[derive(Clone, Debug)]
pub struct VeriFiLoan {
    owner: Address,
    platform: Address,
    next_loan_id: u64,
    loans: HashMap<u64, Loan>,
    off_chain_id_to_loan_id: HashMap<[u8; 32], u64>,
    borrower_loans: HashMap<Address, Vec<u64>>,
    remaining_cents: HashMap<u64, u64>,
    events: Vec<Event>,
    now: u64,
}

impl VeriFiLoan {
    pub fn new(owner: Address, initial_platform: Address) -> Result<Self, LoanError> {
        if initial_platform.is_zero() {
            return Err(LoanError::InvalidAddress);
        }
        let mut registry = Self {
            owner,
            platform: initial_platform,
            next_loan_id: 1,
            loans: HashMap::new(),
            off_chain_id_to_loan_id: HashMap::new(),
            borrower_loans: HashMap::new(),
            remaining_cents: HashMap::new(),
            events: Vec::new(),
            now: 1_700_000_000,
        };
        registry.events.push(Event::OwnershipTransferred {
            previous_owner: Address::zero(),
            new_owner: owner,
        });
        registry.events.push(Event::PlatformUpdated {
            previous_platform: Address::zero(),
            new_platform: initial_platform,
        });
        Ok(registry)
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn transfer_ownership(
        &mut self,
        caller: Address,
        new_owner: Address,
    ) -> Result<(), LoanError> {
        self.only_owner(caller)?;
        if new_owner.is_zero() {
            return Err(LoanError::InvalidAddress);
        }
        self.events.push(Event::OwnershipTransferred {
            previous_owner: self.owner,
            new_owner,
        });
        self.owner = new_owner;
        Ok(())
    }

    pub fn set_platform(
        &mut self,
        caller: Address,
        new_platform: Address,
    ) -> Result<(), LoanError> {
        self.only_owner(caller)?;
        if new_platform.is_zero() {
            return Err(LoanError::InvalidAddress);
        }
        self.events.push(Event::PlatformUpdated {
            previous_platform: self.platform,
            new_platform,
        });
        self.platform = new_platform;
        Ok(())
    }

    /// `off_chain_id` is the id of the off-chain loan record.
    /// `interest_rate_bps` is basis points, e.g. 1299 = 12.99%.
    pub fn create_loan(
        &mut self,
        caller: Address,
        off_chain_id: [u8; 32],
        borrower: Address,
        amount_cents: u64,
        interest_rate_bps: u64,
        term_months: u64,
        monthly_payment_cents: u64,
    ) -> Result<u64, LoanError> {
        self.only_platform(caller)?;
        if borrower.is_zero() {
            return Err(LoanError::InvalidAddress);
        }
        if amount_cents == 0 || term_months == 0 {
            return Err(LoanError::InvalidAmount);
        }
        if self.off_chain_id_to_loan_id.contains_key(&off_chain_id) {
            return Err(LoanError::LoanExists);
        }

        let loan_id = self.next_loan_id;
        self.next_loan_id += 1;
        self.loans.insert(
            loan_id,
            Loan {
                off_chain_id,
                borrower,
                amount_cents,
                interest_rate_bps,
                term_months,
                monthly_payment_cents,
                status: LoanStatus::Pending,
                created_at: self.now,
                approved_at: 0,
                disbursed_at: 0,
            },
        );
        self.off_chain_id_to_loan_id.insert(off_chain_id, loan_id);
        self.borrower_loans.entry(borrower).or_default().push(loan_id);
        self.remaining_cents.insert(loan_id, amount_cents);
        self.events.push(Event::LoanCreated {
            loan_id,
            off_chain_id,
            borrower,
            amount_cents,
            interest_rate_bps,
            term_months,
        });
        Ok(loan_id)
    }

    pub fn approve_loan(&mut self, caller: Address, loan_id: u64) -> Result<(), LoanError> {
        self.only_platform(caller)?;
        self.transition(loan_id, LoanStatus::Pending, LoanStatus::Approved)?;
        self.loans.get_mut(&loan_id).expect("loan exists").approved_at = self.now;
        Ok(())
    }

    pub fn reject_loan(&mut self, caller: Address, loan_id: u64) -> Result<(), LoanError> {
        self.only_platform(caller)?;
        self.transition(loan_id, LoanStatus::Pending, LoanStatus::Rejected)
    }

    pub fn disburse_loan(&mut self, caller: Address, loan_id: u64) -> Result<(), LoanError> {
        self.only_platform(caller)?;
        self.transition(loan_id, LoanStatus::Approved, LoanStatus::Disbursed)?;
        self.loans.get_mut(&loan_id).expect("loan exists").disbursed_at = self.now;
        Ok(())
    }

    pub fn mark_repaid(&mut self, caller: Address, loan_id: u64) -> Result<(), LoanError> {
        self.only_platform(caller)?;
        self.transition(loan_id, LoanStatus::Disbursed, LoanStatus::Repaid)?;
        self.remaining_cents.insert(loan_id, 0);
        Ok(())
    }

    /// BUG: also allows `Defaulted` from `Approved`. Only `Disbursed` should work.
    pub fn mark_defaulted(&mut self, caller: Address, loan_id: u64) -> Result<(), LoanError> {
        self.only_platform(caller)?;
        let prev = {
            let loan = self.loans.get_mut(&loan_id).ok_or(LoanError::LoanNotFound)?;
            if loan.status != LoanStatus::Disbursed && loan.status != LoanStatus::Approved {
                return Err(LoanError::InvalidStatusTransition {
                    from: loan.status,
                    to: LoanStatus::Defaulted,
                });
            }
            let prev = loan.status;
            loan.status = LoanStatus::Defaulted;
            prev
        };
        self.events.push(Event::LoanStatusUpdated {
            loan_id,
            previous_status: prev,
            new_status: LoanStatus::Defaulted,
        });
        Ok(())
    }

    /// Record a repayment against a disbursed loan. See README "Your task".
    ///
    /// `loans`, `remaining_cents`, and `events` are separate fields. Copy what you
    /// need out of a loan before you push an event.
    pub fn record_payment(
        &mut self,
        _caller: Address,
        _loan_id: u64,
        _amount_cents: u64,
    ) -> Result<(), LoanError> {
        // TODO: candidate implements
        todo!("record_payment")
    }

    pub fn remaining_of(&self, _loan_id: u64) -> Result<u64, LoanError> {
        // TODO: candidate implements
        todo!("remaining_of")
    }

    pub fn borrower_loans(&self, borrower: Address) -> &[u64] {
        self.borrower_loans
            .get(&borrower)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn get_loan(&self, loan_id: u64) -> Result<&Loan, LoanError> {
        self.loans.get(&loan_id).ok_or(LoanError::LoanNotFound)
    }

    fn only_owner(&self, caller: Address) -> Result<(), LoanError> {
        if caller != self.owner {
            return Err(LoanError::Unauthorized);
        }
        Ok(())
    }

    fn only_platform(&self, caller: Address) -> Result<(), LoanError> {
        if caller != self.platform && caller != self.owner {
            return Err(LoanError::Unauthorized);
        }
        Ok(())
    }

    fn transition(
        &mut self,
        loan_id: u64,
        expected: LoanStatus,
        next: LoanStatus,
    ) -> Result<(), LoanError> {
        {
            let loan = self.loans.get_mut(&loan_id).ok_or(LoanError::LoanNotFound)?;
            if loan.status != expected {
                return Err(LoanError::InvalidStatusTransition {
                    from: loan.status,
                    to: next,
                });
            }
            loan.status = next;
        }
        self.events.push(Event::LoanStatusUpdated {
            loan_id,
            previous_status: expected,
            new_status: next,
        });
        Ok(())
    }
}
