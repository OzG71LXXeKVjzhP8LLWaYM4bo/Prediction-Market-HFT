use alloy_primitives::{Address, U256};
use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::{sol, SolStruct};
use pmhft_common::{PmhftError, Result};

// Define the EIP-712 structs matching Polymarket's CTF Exchange contract.
sol! {
    #[derive(Debug)]
    struct Order {
        uint256 salt;
        address maker;
        address signer;
        address taker;
        uint256 tokenId;
        uint256 makerAmount;
        uint256 takerAmount;
        uint256 expiration;
        uint256 nonce;
        uint256 feeRateBps;
        uint8 side;
        uint8 signatureType;
    }
}

/// EIP-712 domain separator for Polymarket's CTF Exchange.
fn polymarket_domain(chain_id: u64, exchange_address: Address) -> alloy_sol_types::Eip712Domain {
    alloy_sol_types::eip712_domain! {
        name: "Polymarket CTF Exchange",
        version: "1",
        chain_id: chain_id,
        verifying_contract: exchange_address,
    }
}

/// Signs Polymarket CLOB orders using EIP-712 typed data.
pub struct PolymarketOrderSigner {
    signer: PrivateKeySigner,
    domain: alloy_sol_types::Eip712Domain,
    maker_address: Address,
}

impl PolymarketOrderSigner {
    pub fn new(private_key_hex: &str, chain_id: u64, exchange_address: &str) -> Result<Self> {
        let signer: PrivateKeySigner = private_key_hex
            .parse()
            .map_err(|e| PmhftError::Eip712Signing(format!("Invalid private key: {}", e)))?;
        let maker_address = signer.address();
        let exchange_addr: Address = exchange_address
            .parse()
            .map_err(|e| PmhftError::Eip712Signing(format!("Invalid exchange address: {}", e)))?;
        let domain = polymarket_domain(chain_id, exchange_addr);

        Ok(Self {
            signer,
            domain,
            maker_address,
        })
    }

    pub fn maker_address(&self) -> Address {
        self.maker_address
    }

    /// Sign an order for the Polymarket CLOB.
    ///
    /// # Arguments
    /// * `token_id` - The outcome token ID (large numeric string).
    /// * `maker_amount` - Collateral (USDC) amount in smallest units (6 decimals).
    /// * `taker_amount` - Outcome token amount in smallest units.
    /// * `side` - 0 = BUY, 1 = SELL.
    /// * `nonce` - Order nonce.
    /// * `fee_rate_bps` - Fee rate in basis points.
    /// * `expiration` - Unix timestamp for order expiration, or U256::MAX for no expiry.
    ///
    /// Returns (Order struct, signature bytes).
    pub async fn sign_order(
        &self,
        token_id: U256,
        maker_amount: U256,
        taker_amount: U256,
        side: u8,
        nonce: U256,
        fee_rate_bps: U256,
        expiration: U256,
    ) -> Result<(Order, Vec<u8>)> {
        let salt = U256::from(rand::random::<u128>());

        let order = Order {
            salt,
            maker: self.maker_address,
            signer: self.maker_address,
            taker: Address::ZERO,
            tokenId: token_id,
            makerAmount: maker_amount,
            takerAmount: taker_amount,
            expiration,
            nonce,
            feeRateBps: fee_rate_bps,
            side,
            signatureType: 0, // EOA signature
        };

        let signing_hash = order.eip712_signing_hash(&self.domain);

        let signature = self
            .signer
            .sign_hash(&signing_hash)
            .await
            .map_err(|e| PmhftError::Eip712Signing(format!("Signing failed: {}", e)))?;

        Ok((order, signature.as_bytes().to_vec()))
    }
}
