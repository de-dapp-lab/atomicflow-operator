use crate::domain::error::ServiceError;
use crate::domain::transaction::Transaction;
use crate::infrastructure::repository::wallet::WalletRepository;
use intmax::service::builder::ServiceBuilder;
use intmax::service::functions::bulk_mint;
use intmax::utils::key_management::memory::{SerializableWalletOnMemory, WalletOnMemory};
use intmax::utils::key_management::types::Wallet;
use intmax_rollup_interface::intmax_zkp_core::plonky2::plonk::config::{
    GenericConfig, PoseidonGoldilocksConfig,
};
use intmax_rollup_interface::intmax_zkp_core::sparse_merkle_tree::goldilocks_poseidon::WrappedHashOut;
use intmax_rollup_interface::intmax_zkp_core::transaction::asset::{ContributedAsset, TokenKind};
use intmax_rollup_interface::intmax_zkp_core::zkdsa::account::{Account, Address};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

const D: usize = 2;
type C = PoseidonGoldilocksConfig;
pub type F = <C as GenericConfig<D>>::F;

#[derive(Clone)]
pub struct IntmaxService {
    intmax_service_builder: ServiceBuilder,
    wallet_repo: WalletRepository,
}

impl IntmaxService {
    pub fn new(intmax_service_builder: ServiceBuilder, wallet_repo: WalletRepository) -> Self {
        Self {
            intmax_service_builder,
            wallet_repo,
        }
    }

    pub async fn bulk_transfer(
        &self,
        assets: &str,
        payer_address: &str,
        transactions: Vec<Transaction>,
    ) -> anyhow::Result<()> {
        let raw: SerializableWalletOnMemory = serde_json::from_str(assets)?;
        let mut result = HashMap::new();
        for value in raw.data.into_iter() {
            result.insert(value.account.address, value);
        }

        let mut wallet = WalletOnMemory {
            data: result,
            default_account: raw.default_account,
            wallet_file_path: self.wallet_repo.wallet_file_path.clone(),
        };

        let payer_address = Address::from_str(payer_address)?;

        let mut distribution_list = vec![];

        transactions
            .into_iter()
            .try_for_each(|tx| -> anyhow::Result<()> {
                let asset = ContributedAsset {
                    kind: TokenKind {
                        contract_address: Address::from_str(&tx.payer_address)?,
                        variable_index: tx.token_address.to_string().parse::<u8>()?.into(),
                    },
                    receiver_address: Address::from_str(&tx.receiver_address)?,
                    amount: tx.amount,
                };
                distribution_list.push(asset);
                Ok(())
            })?;

        let bulk_mint_result = bulk_mint(
            &self.intmax_service_builder,
            &mut wallet,
            payer_address,
            distribution_list,
            false,
        )
        .await;

        match bulk_mint_result {
            Err(err) => {
                if err.to_string() == "output asset amount is too much" {
                    anyhow::bail!(ServiceError::FailedTransaction)
                } else {
                    anyhow::bail!(err)
                }
            }
            _ => Ok(()),
        }
    }

    pub async fn new_encoded_wallet(&self) -> anyhow::Result<(String, String)> {
        let path = PathBuf::new();
        let mut wallet = WalletOnMemory::new(path, "password".to_string());
        let account = Account::new(*WrappedHashOut::rand());
        self.intmax_service_builder
            .register_account(account.public_key)
            .await;
        wallet.add_account(account)?;

        let encode_wallet = self.wallet_repo.encode_wallet(wallet)?;
        Ok((encode_wallet, account.address.to_string()))
    }
}