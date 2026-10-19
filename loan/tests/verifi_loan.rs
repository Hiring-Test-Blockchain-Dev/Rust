use verifi_loan::{Address, Event, LoanError, LoanStatus, VeriFiLoan};

fn addr(n: u8) -> Address {
    let mut bytes = [0u8; 20];
    bytes[19] = n;
    Address::new(bytes)
}

fn id(label: &str) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    let raw = label.as_bytes();
    let n = raw.len().min(32);
    bytes[..n].copy_from_slice(&raw[..n]);
    bytes
}

struct World {
    loan: VeriFiLoan,
    platform: Address,
    borrower: Address,
    stranger: Address,
}

fn world() -> World {
    let owner = addr(1);
    let platform = addr(2);
    let borrower = addr(3);
    let stranger = addr(4);
    let loan = VeriFiLoan::new(owner, platform).unwrap();
    World {
        loan,
        platform,
        borrower,
        stranger,
    }
}

fn disburse(world: &mut World, amount_cents: u64, label: &str) {
    world
        .loan
        .create_loan(
            world.platform,
            id(label),
            world.borrower,
            amount_cents,
            1299,
            12,
            900,
        )
        .unwrap();
    world.loan.approve_loan(world.platform, 1).unwrap();
    world.loan.disburse_loan(world.platform, 1).unwrap();
}

fn assert_err<T: std::fmt::Debug>(result: Result<T, LoanError>, expected: LoanError) {
    match result {
        Err(actual) => assert_eq!(actual, expected),
        Ok(value) => panic!("expected {expected:?}, got Ok({value:?})"),
    }
}

#[test]
fn creates_a_loan() {
    let mut world = world();
    let off_chain_id = id("loan-uuid-1");
    let loan_id = world
        .loan
        .create_loan(world.platform, off_chain_id, world.borrower, 100_000, 1299, 12, 8900)
        .unwrap();
    assert_eq!(loan_id, 1);
    assert!(world.loan.events().contains(&Event::LoanCreated {
        loan_id: 1,
        off_chain_id,
        borrower: world.borrower,
        amount_cents: 100_000,
        interest_rate_bps: 1299,
        term_months: 12,
    }));
    let stored = world.loan.get_loan(1).unwrap();
    assert_eq!(stored.borrower, world.borrower);
    assert_eq!(stored.status, LoanStatus::Pending);
}

#[test]
fn approve_then_disburse() {
    let mut world = world();
    world
        .loan
        .create_loan(world.platform, id("loan-uuid-2"), world.borrower, 50_000, 1299, 6, 8500)
        .unwrap();
    world.loan.approve_loan(world.platform, 1).unwrap();
    world.loan.disburse_loan(world.platform, 1).unwrap();
    assert_eq!(world.loan.get_loan(1).unwrap().status, LoanStatus::Disbursed);
}

#[test]
fn non_platform_cant_create() {
    let mut world = world();
    assert_err(
        world.loan.create_loan(
            world.borrower,
            id("loan-uuid-3"),
            world.borrower,
            1000,
            100,
            1,
            100,
        ),
        LoanError::Unauthorized,
    );
}

#[test]
fn remaining_of_returns_the_outstanding_balance() {
    let mut world = world();
    disburse(&mut world, 10_000, "loan-pay-balance");
    assert_eq!(world.loan.remaining_of(1).unwrap(), 10_000);
}

#[test]
fn remaining_of_errors_when_the_loan_does_not_exist() {
    let world = world();
    assert_err(world.loan.remaining_of(99), LoanError::LoanNotFound);
}

#[test]
fn borrower_can_pay_and_remaining_drops() {
    let mut world = world();
    disburse(&mut world, 10_000, "loan-pay-partial");
    world.loan.record_payment(world.borrower, 1, 4_000).unwrap();
    assert!(world.loan.events().contains(&Event::PaymentRecorded {
        loan_id: 1,
        payer: world.borrower,
        amount_cents: 4_000,
        remaining_cents: 6_000,
    }));
    assert_eq!(world.loan.remaining_of(1).unwrap(), 6_000);
    assert_eq!(world.loan.get_loan(1).unwrap().status, LoanStatus::Disbursed);
}

#[test]
fn platform_can_record_a_payment() {
    let mut world = world();
    disburse(&mut world, 10_000, "loan-pay-platform");
    world.loan.record_payment(world.platform, 1, 1_000).unwrap();
    assert_eq!(world.loan.remaining_of(1).unwrap(), 9_000);
}

#[test]
fn final_payment_marks_the_loan_repaid() {
    let mut world = world();
    disburse(&mut world, 10_000, "loan-pay-final");
    world.loan.record_payment(world.borrower, 1, 10_000).unwrap();
    assert!(world.loan.events().contains(&Event::PaymentRecorded {
        loan_id: 1,
        payer: world.borrower,
        amount_cents: 10_000,
        remaining_cents: 0,
    }));
    assert!(world.loan.events().contains(&Event::LoanStatusUpdated {
        loan_id: 1,
        previous_status: LoanStatus::Disbursed,
        new_status: LoanStatus::Repaid,
    }));
    assert_eq!(world.loan.get_loan(1).unwrap().status, LoanStatus::Repaid);
}

#[test]
fn rejects_a_stranger() {
    let mut world = world();
    disburse(&mut world, 10_000, "loan-pay-stranger");
    assert_err(
        world.loan.record_payment(world.stranger, 1, 100),
        LoanError::Unauthorized,
    );
}

#[test]
fn rejects_zero_and_overpayment() {
    let mut world = world();
    disburse(&mut world, 10_000, "loan-pay-bounds");
    assert_err(
        world.loan.record_payment(world.borrower, 1, 0),
        LoanError::InvalidAmount,
    );
    assert_err(
        world.loan.record_payment(world.borrower, 1, 10_001),
        LoanError::InvalidAmount,
    );
}

#[test]
fn rejects_payment_before_disbursal() {
    let mut world = world();
    world
        .loan
        .create_loan(
            world.platform,
            id("loan-not-disbursed"),
            world.borrower,
            5_000,
            1299,
            6,
            900,
        )
        .unwrap();
    let err = world.loan.record_payment(world.borrower, 1, 100).unwrap_err();
    assert!(matches!(err, LoanError::InvalidStatusTransition { .. }));
}

#[test]
fn defaults_a_disbursed_loan() {
    let mut world = world();
    world
        .loan
        .create_loan(
            world.platform,
            id("loan-default-ok"),
            world.borrower,
            5_000,
            1299,
            6,
            900,
        )
        .unwrap();
    world.loan.approve_loan(world.platform, 1).unwrap();
    world.loan.disburse_loan(world.platform, 1).unwrap();
    world.loan.mark_defaulted(world.platform, 1).unwrap();
    assert_eq!(world.loan.get_loan(1).unwrap().status, LoanStatus::Defaulted);
}

#[test]
fn cannot_default_an_approved_loan() {
    let mut world = world();
    world
        .loan
        .create_loan(
            world.platform,
            id("loan-default-approved"),
            world.borrower,
            5_000,
            1299,
            6,
            900,
        )
        .unwrap();
    world.loan.approve_loan(world.platform, 1).unwrap();
    let err = world.loan.mark_defaulted(world.platform, 1).unwrap_err();
    assert!(matches!(err, LoanError::InvalidStatusTransition { .. }));
}
