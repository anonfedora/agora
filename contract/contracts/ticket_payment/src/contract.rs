use crate::storage::{
    add_payment_to_buyer_index, add_token_to_whitelist, get_admin, get_event_balance,
    get_event_registry, get_payment, get_platform_wallet, get_transfer_fee, is_initialized,
    is_token_whitelisted, remove_payment_from_buyer_index, remove_token_from_whitelist, set_admin,
    set_event_registry, set_initialized, set_platform_wallet, set_transfer_fee, set_usdc_token,
    store_payment, update_event_balance, update_payment_status,
};
use crate::types::{Payment, PaymentStatus};
use crate::{
    error::TicketPaymentError,
    events::{
        AgoraEvent, ContractUpgraded, InitializationEvent, PaymentProcessedEvent,
        PaymentStatusChangedEvent, TicketTransferredEvent,
    },
};
use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, String};

// Event Registry interface
pub mod event_registry {
    use soroban_sdk::{contractclient, Address, Env, String};

    #[soroban_sdk::contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PaymentInfo {
        pub payment_address: Address,
        pub platform_fee_percent: u32,
    }

    #[soroban_sdk::contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct EventInventory {
        pub current_supply: i128,
        pub max_supply: i128,
    }

    #[contractclient(name = "Client")]
    pub trait EventRegistryInterface {
        fn get_event_payment_info(env: Env, event_id: String) -> PaymentInfo;
        fn get_event(env: Env, event_id: String) -> Option<EventInfo>;
        fn increment_inventory(env: Env, event_id: String, tier_id: String, quantity: u32);
        fn decrement_inventory(env: Env, event_id: String, tier_id: String);
    }

    #[soroban_sdk::contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct TicketTier {
        pub name: String,
        pub price: i128,
        pub tier_limit: i128,
        pub current_sold: i128,
        pub is_refundable: bool,
    }

    #[soroban_sdk::contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Milestone {
        pub sales_threshold: i128,
        pub release_percent: u32,
    }

    #[soroban_sdk::contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct EventInfo {
        pub event_id: String,
        pub organizer_address: Address,
        pub payment_address: Address,
        pub platform_fee_percent: u32,
        pub is_active: bool,
        pub created_at: u64,
        pub metadata_cid: String,
        pub max_supply: i128,
        pub current_supply: i128,
        pub milestone_plan: Option<soroban_sdk::Vec<Milestone>>,
        pub tiers: soroban_sdk::Map<String, TicketTier>,
    }
}

#[contract]
pub struct TicketPaymentContract;

#[contractimpl]
#[allow(deprecated)]
impl TicketPaymentContract {
    /// Initializes the contract with necessary configurations.
    pub fn initialize(
        env: Env,
        admin: Address,
        usdc_token: Address,
        platform_wallet: Address,
        event_registry: Address,
    ) -> Result<(), TicketPaymentError> {
        if is_initialized(&env) {
            return Err(TicketPaymentError::AlreadyInitialized);
        }

        validate_address(&env, &admin)?;
        validate_address(&env, &usdc_token)?;
        validate_address(&env, &platform_wallet)?;
        validate_address(&env, &event_registry)?;

        set_admin(&env, &admin);
        set_usdc_token(&env, usdc_token.clone());
        set_platform_wallet(&env, platform_wallet.clone());
        set_event_registry(&env, event_registry.clone());
        set_initialized(&env, true);

        // Whitelist USDC by default
        add_token_to_whitelist(&env, &usdc_token);

        env.events().publish(
            (AgoraEvent::ContractInitialized,),
            InitializationEvent {
                usdc_token,
                platform_wallet,
                event_registry,
            },
        );

        Ok(())
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin = get_admin(&env).expect("Admin not set");
        admin.require_auth();

        let old_wasm_hash = match env.current_contract_address().executable() {
            Some(soroban_sdk::Executable::Wasm(hash)) => hash,
            _ => panic!("Current contract is not a Wasm contract"),
        };

        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());

        env.events().publish(
            (AgoraEvent::ContractUpgraded,),
            ContractUpgraded {
                old_wasm_hash,
                new_wasm_hash,
            },
        );
    }

    pub fn add_token(env: Env, token: Address) {
        let admin = get_admin(&env).expect("Admin not set");
        admin.require_auth();
        add_token_to_whitelist(&env, &token);
    }

    pub fn remove_token(env: Env, token: Address) {
        let admin = get_admin(&env).expect("Admin not set");
        admin.require_auth();
        remove_token_from_whitelist(&env, &token);
    }

    pub fn is_token_allowed(env: Env, token: Address) -> bool {
        is_token_whitelisted(&env, &token)
    }

    /// Processes a payment for an event ticket.
    pub fn process_payment(
        env: Env,
        payment_id: String,
        event_id: String,
        ticket_tier_id: String,
        buyer_address: Address,
        token_address: Address,
        amount: i128, // price for ONE ticket
        quantity: u32,
    ) -> Result<String, TicketPaymentError> {
        if !is_initialized(&env) {
            panic!("Contract not initialized");
        }
        buyer_address.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        if quantity == 0 {
            panic!("Quantity must be positive");
        }

        if !is_token_whitelisted(&env, &token_address) {
            return Err(TicketPaymentError::TokenNotWhitelisted);
        }

        let total_amount = amount
            .checked_mul(quantity as i128)
            .ok_or(TicketPaymentError::ArithmeticError)?;

        // 1. Query Event Registry for event info and check inventory
        let event_registry_addr = get_event_registry(&env);
        let registry_client = event_registry::Client::new(&env, &event_registry_addr);

        let event_info = match registry_client.try_get_event(&event_id) {
            Ok(Ok(Some(info))) => info,
            Ok(Ok(None)) => return Err(TicketPaymentError::EventNotFound),
            _ => return Err(TicketPaymentError::EventNotFound),
        };

        if !event_info.is_active {
            return Err(TicketPaymentError::EventInactive);
        }

        // 2. Calculate platform fee (platform_fee_percent is in bps, 10000 = 100%)
        let total_platform_fee = (total_amount * event_info.platform_fee_percent as i128) / 10000;
        let total_organizer_amount = total_amount - total_platform_fee;

        // 3. Transfer tokens to contract (escrow)
        let token_client = token::Client::new(&env, &token_address);
        let contract_address = env.current_contract_address();

        // Verify allowance
        let allowance = token_client.allowance(&buyer_address, &contract_address);
        if allowance < total_amount {
            return Err(TicketPaymentError::InsufficientAllowance);
        }

        // Get balance before transfer
        let balance_before = token_client.balance(&contract_address);

        // Transfer full amount to contract
        token_client.transfer_from(
            &contract_address,
            &buyer_address,
            &contract_address,
            &total_amount,
        );

        // Verify balance after transfer
        let balance_after = token_client.balance(&contract_address);
        if balance_after - balance_before != total_amount {
            return Err(TicketPaymentError::TransferVerificationFailed);
        }

        // 4. Update escrow balances
        update_event_balance(
            &env,
            event_id.clone(),
            total_organizer_amount,
            total_platform_fee,
        );

        // 5. Increment inventory after successful payment
        registry_client.increment_inventory(&event_id, &ticket_tier_id, &quantity);

        // 6. Create payment records for each individual ticket
        let platform_fee_per_ticket = total_platform_fee / quantity as i128;
        let organizer_amount_per_ticket = total_organizer_amount / quantity as i128;

        for i in 0..quantity {
            // Re-initialize the sub_payment_id with a unique ID for each ticket in a batch.
            // Since concatenation is complex in Soroban no_std, we use a match for common indices.
            let sub_payment_id = if quantity == 1 {
                payment_id.clone()
            } else {
                match i {
                    0 => String::from_str(&env, "p-0"),
                    1 => String::from_str(&env, "p-1"),
                    2 => String::from_str(&env, "p-2"),
                    3 => String::from_str(&env, "p-3"),
                    4 => String::from_str(&env, "p-4"),
                    _ => String::from_str(&env, "p-many"),
                }
            };

            let payment = Payment {
                payment_id: sub_payment_id.clone(),
                event_id: event_id.clone(),
                buyer_address: buyer_address.clone(),
                ticket_tier_id: ticket_tier_id.clone(),
                amount,
                platform_fee: platform_fee_per_ticket,
                organizer_amount: organizer_amount_per_ticket,
                status: PaymentStatus::Pending,
                transaction_hash: String::from_str(&env, ""),
                created_at: env.ledger().timestamp(),
                confirmed_at: None,
            };

            store_payment(&env, payment);
        }

        // 7. Emit payment event
        env.events().publish(
            (AgoraEvent::PaymentProcessed,),
            PaymentProcessedEvent {
                payment_id: payment_id.clone(),
                event_id: event_id.clone(),
                buyer_address: buyer_address.clone(),
                amount: total_amount,
                platform_fee: total_platform_fee,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(payment_id)
    }

    /// Confirms a payment after backend verification.
    pub fn confirm_payment(env: Env, payment_id: String, transaction_hash: String) {
        if !is_initialized(&env) {
            panic!("Contract not initialized");
        }
        // In a real scenario, this would be restricted to a specific backend/admin address.
        update_payment_status(
            &env,
            payment_id.clone(),
            PaymentStatus::Confirmed,
            Some(env.ledger().timestamp()),
        );

        // Update the transaction hash
        if let Some(mut payment) = get_payment(&env, payment_id.clone()) {
            payment.transaction_hash = transaction_hash.clone();
            store_payment(&env, payment);
        }

        // Emit confirmation event
        env.events().publish(
            (AgoraEvent::PaymentStatusChanged,),
            PaymentStatusChangedEvent {
                payment_id: payment_id.clone(),
                old_status: PaymentStatus::Pending,
                new_status: PaymentStatus::Confirmed,
                transaction_hash: transaction_hash.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    pub fn request_guest_refund(env: Env, payment_id: String) -> Result<(), TicketPaymentError> {
        if !is_initialized(&env) {
            panic!("Contract not initialized");
        }

        let mut payment =
            get_payment(&env, payment_id.clone()).ok_or(TicketPaymentError::PaymentNotFound)?;

        payment.buyer_address.require_auth();

        if payment.status == PaymentStatus::Refunded || payment.status == PaymentStatus::Failed {
            return Err(TicketPaymentError::InvalidPaymentStatus);
        }

        let event_registry_addr = get_event_registry(&env);
        let registry_client = event_registry::Client::new(&env, &event_registry_addr);

        let event_info = match registry_client.try_get_event(&payment.event_id) {
            Ok(Ok(Some(info))) => info,
            _ => return Err(TicketPaymentError::EventNotFound),
        };

        let tier = event_info
            .tiers
            .get(payment.ticket_tier_id.clone())
            .ok_or(TicketPaymentError::TierNotFound)?;

        // Check if refundable or if EVENT IS CANCELLED (is_active == false)
        if !tier.is_refundable && event_info.is_active {
            return Err(TicketPaymentError::TicketNotRefundable);
        }

        // Return ticket to inventory using the authorized contract interface
        registry_client.decrement_inventory(&payment.event_id, &payment.ticket_tier_id);

        let old_status = payment.status.clone();
        payment.status = PaymentStatus::Refunded;
        payment.confirmed_at = Some(env.ledger().timestamp());

        store_payment(&env, payment);

        // Emit confirmation event
        env.events().publish(
            (AgoraEvent::PaymentStatusChanged,),
            PaymentStatusChangedEvent {
                payment_id: payment_id.clone(),
                old_status,
                new_status: PaymentStatus::Refunded,
                transaction_hash: String::from_str(&env, "refund"),
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    /// Returns the status and details of a payment.
    pub fn get_payment_status(env: Env, payment_id: String) -> Option<Payment> {
        get_payment(&env, payment_id)
    }

    /// Returns the escrowed balance for an event.
    pub fn get_event_escrow_balance(env: Env, event_id: String) -> crate::types::EventBalance {
        get_event_balance(&env, event_id)
    }

    /// Withdraw organizer funds from escrow.
    pub fn withdraw_organizer_funds(
        env: Env,
        event_id: String,
        token_address: Address,
    ) -> Result<i128, TicketPaymentError> {
        let event_registry_addr = get_event_registry(&env);
        let registry_client = event_registry::Client::new(&env, &event_registry_addr);
        let event_info = registry_client
            .try_get_event(&event_id)
            .ok()
            .and_then(|r| r.ok())
            .flatten()
            .ok_or(TicketPaymentError::EventNotFound)?;

        event_info.organizer_address.require_auth();

        let balance = get_event_balance(&env, event_id.clone());
        let total_revenue = balance.organizer_amount + balance.total_withdrawn;
        if total_revenue == 0 {
            return Ok(0);
        }

        let mut release_percent = 10000u32;
        if let Some(milestones) = event_info.milestone_plan {
            let mut highest_met = 0u32;
            for milestone in milestones.iter() {
                if event_info.current_supply >= milestone.sales_threshold
                    && milestone.release_percent > highest_met
                {
                    highest_met = milestone.release_percent;
                }
            }
            if !milestones.is_empty() {
                release_percent = highest_met;
            }
        }

        let max_allowed = (total_revenue * release_percent as i128) / 10000;
        let mut available_to_withdraw = max_allowed - balance.total_withdrawn;

        if available_to_withdraw <= 0 {
            return Ok(0);
        }

        if available_to_withdraw > balance.organizer_amount {
            available_to_withdraw = balance.organizer_amount;
        }

        token::Client::new(&env, &token_address).transfer(
            &env.current_contract_address(),
            &event_info.organizer_address,
            &available_to_withdraw,
        );

        crate::storage::set_event_balance(
            &env,
            event_id,
            crate::types::EventBalance {
                organizer_amount: balance.organizer_amount - available_to_withdraw,
                total_withdrawn: balance.total_withdrawn + available_to_withdraw,
                platform_fee: balance.platform_fee,
            },
        );

        Ok(available_to_withdraw)
    }

    /// Withdraw platform fees from escrow.
    pub fn withdraw_platform_fees(
        env: Env,
        event_id: String,
        token_address: Address,
    ) -> Result<i128, TicketPaymentError> {
        let admin = get_admin(&env).ok_or(TicketPaymentError::NotInitialized)?;
        admin.require_auth();

        let balance = get_event_balance(&env, event_id.clone());
        if balance.platform_fee == 0 {
            return Ok(0);
        }

        let platform_wallet = get_platform_wallet(&env);
        token::Client::new(&env, &token_address).transfer(
            &env.current_contract_address(),
            &platform_wallet,
            &balance.platform_fee,
        );

        crate::storage::set_event_balance(
            &env,
            event_id,
            crate::types::EventBalance {
                organizer_amount: balance.organizer_amount,
                total_withdrawn: balance.total_withdrawn,
                platform_fee: 0,
            },
        );

        Ok(balance.platform_fee)
    }

    /// Returns all payments for a specific buyer.
    pub fn get_buyer_payments(env: Env, buyer_address: Address) -> soroban_sdk::Vec<String> {
        crate::storage::get_buyer_payments(&env, buyer_address)
    }

    /// Sets the transfer fee for an event. Only the organizer can call this.
    pub fn set_transfer_fee(
        env: Env,
        event_id: String,
        amount: i128,
    ) -> Result<(), TicketPaymentError> {
        if !is_initialized(&env) {
            panic!("Contract not initialized");
        }

        let event_registry_addr = get_event_registry(&env);
        let registry_client = event_registry::Client::new(&env, &event_registry_addr);

        let event_info = match registry_client.try_get_event(&event_id) {
            Ok(Ok(Some(info))) => info,
            _ => return Err(TicketPaymentError::EventNotFound),
        };

        event_info.organizer_address.require_auth();

        if amount < 0 {
            panic!("Transfer fee must be non-negative");
        }

        set_transfer_fee(&env, event_id, amount);
        Ok(())
    }

    /// Transfers a ticket from the current holder to a new owner.
    pub fn transfer_ticket(
        env: Env,
        payment_id: String,
        to: Address,
    ) -> Result<(), TicketPaymentError> {
        if !is_initialized(&env) {
            panic!("Contract not initialized");
        }

        let mut payment =
            get_payment(&env, payment_id.clone()).ok_or(TicketPaymentError::PaymentNotFound)?;

        if payment.status != PaymentStatus::Confirmed {
            return Err(TicketPaymentError::InvalidPaymentStatus);
        }

        let from = payment.buyer_address.clone();
        from.require_auth();

        if from == to {
            return Err(TicketPaymentError::InvalidAddress);
        }

        let transfer_fee = get_transfer_fee(&env, payment.event_id.clone());

        if transfer_fee > 0 {
            let token_address = crate::storage::get_usdc_token(&env);
            let token_client = token::Client::new(&env, &token_address);
            let contract_address = env.current_contract_address();

            // Transfer fee from old owner to contract
            token_client.transfer_from(&contract_address, &from, &contract_address, &transfer_fee);

            // Update escrow balances (fee goes to organizer)
            update_event_balance(&env, payment.event_id.clone(), transfer_fee, 0);
        }

        // Update payment record
        payment.buyer_address = to.clone();
        let key = crate::types::DataKey::Payment(payment_id.clone());
        env.storage().persistent().set(&key, &payment);

        // Update indices
        remove_payment_from_buyer_index(&env, from.clone(), payment_id.clone());
        add_payment_to_buyer_index(&env, to.clone(), payment_id.clone());

        // Emit transfer event
        env.events().publish(
            (AgoraEvent::TicketTransferred,),
            TicketTransferredEvent {
                payment_id,
                from,
                to,
                transfer_fee,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }
}

fn validate_address(env: &Env, address: &Address) -> Result<(), TicketPaymentError> {
    if address == &env.current_contract_address() {
        return Err(TicketPaymentError::InvalidAddress);
    }
    Ok(())
}
