use crate::state::{account_viewkey_r, address_in_account_r, validate_address_permit};
use crate::{
    handle::decay_factor,
    state::{
        account_r, account_total_claimed_r, claim_status_r, config_r, decay_claimed_r,
        total_claimed_r, validate_account_permit,
    },
};
use cosmwasm_std::{Api, Extern, HumanAddr, Querier, StdResult, Storage, Uint128};
use query_authentication::viewing_keys::ViewingKey;
use shade_protocol::airdrop::account::{AccountKey, AddressProofPermit};
use shade_protocol::airdrop::errors::invalid_viewing_key;
use shade_protocol::airdrop::AccountVerification;
use shade_protocol::{
    airdrop::{account::AccountPermit, claim_info::RequiredTask, QueryAnswer},
    utils::math::{div, mult},
};

pub fn config<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<QueryAnswer> {
    Ok(QueryAnswer::Config {
        config: config_r(&deps.storage).load()?,
    })
}

pub fn dates<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    current_date: Option<u64>,
) -> StdResult<QueryAnswer> {
    let config = config_r(&deps.storage).load()?;
    Ok(QueryAnswer::Dates {
        start: config.start_date,
        end: config.end_date,
        decay_start: config.decay_start,
        decay_factor: current_date.map(|date| Uint128(100) * decay_factor(date, &config)),
    })
}

pub fn total_claimed<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<QueryAnswer> {
    let claimed: Uint128;
    let total_claimed = total_claimed_r(&deps.storage).load()?;
    if decay_claimed_r(&deps.storage).load()? {
        claimed = total_claimed;
    } else {
        let config = config_r(&deps.storage).load()?;
        claimed = mult(
            div(total_claimed, config.query_rounding)?,
            config.query_rounding,
        );
    }
    Ok(QueryAnswer::TotalClaimed { claimed })
}

fn account_information<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    account_address: HumanAddr,
    current_date: Option<u64>,
) -> StdResult<QueryAnswer> {
    let account = account_r(&deps.storage).load(account_address.to_string().as_bytes())?;

    // Calculate eligible tasks
    let config = config_r(&deps.storage).load()?;
    let mut finished_tasks: Vec<RequiredTask> = vec![];
    let mut completed_percentage = Uint128::zero();
    let mut unclaimed_percentage = Uint128::zero();
    for (index, task) in config.task_claim.iter().enumerate() {
        // Check if task has been completed
        let state = claim_status_r(&deps.storage, index)
            .may_load(account_address.to_string().as_bytes())?;

        match state {
            // Ignore if none
            None => {}
            Some(claimed) => {
                finished_tasks.push(task.clone());
                if !claimed {
                    unclaimed_percentage += task.percent;
                } else {
                    completed_percentage += task.percent;
                }
            }
        }
    }

    let mut unclaimed: Uint128;

    if unclaimed_percentage == Uint128(100) {
        unclaimed = account.total_claimable;
    } else {
        unclaimed = unclaimed_percentage.multiply_ratio(account.total_claimable, Uint128(100));
    }

    if let Some(time) = current_date {
        unclaimed = unclaimed * decay_factor(time, &config);
    }

    Ok(QueryAnswer::Account {
        total: account.total_claimable,
        claimed: account_total_claimed_r(&deps.storage)
            .load(account_address.to_string().as_bytes())?,
        unclaimed,
        finished_tasks,
        addresses: account.addresses,
    })
}

pub fn account<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    permit: AccountPermit,
    current_date: Option<u64>,
) -> StdResult<QueryAnswer> {
    let config = config_r(&deps.storage).load()?;
    account_information(
        deps,
        validate_account_permit(deps, &permit, config.contract)?,
        current_date,
    )
}

pub fn account_with_key<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    account: HumanAddr,
    key: String,
    current_date: Option<u64>,
) -> StdResult<QueryAnswer> {
    // Validate address
    let stored_hash = account_viewkey_r(&deps.storage).load(account.to_string().as_bytes())?;

    if !AccountKey(key).compare(&stored_hash) {
        return Err(invalid_viewing_key());
    }

    account_information(deps, account, current_date)
}
