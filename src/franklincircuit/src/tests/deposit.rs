use super::utils::*;


use crate::operation::*;
use crate::utils::*;



use ff::{BitIterator, Field, PrimeField, PrimeFieldRepr};

use franklin_crypto::circuit::float_point::convert_to_float;



use franklin_crypto::jubjub::JubjubEngine;
use franklinmodels::circuit::account::{
    Balance, CircuitAccount, CircuitAccountTree, CircuitBalanceTree,
};
use franklinmodels::params as franklin_constants;
use merkle_tree::hasher::Hasher;
use merkle_tree::PedersenHasher;
use pairing::bn256::*;


pub struct DepositData {
    pub amount: u128,
    pub fee: u128,
    pub token: u32,
    pub account_address: u32,
    pub new_pub_x: Fr,
    pub new_pub_y: Fr,
}
pub struct DepositWitness<E: JubjubEngine> {
    pub before: OperationBranch<E>,
    pub after: OperationBranch<E>,
    pub args: OperationArguments<E>,
    pub before_root: Option<E::Fr>,
    pub after_root: Option<E::Fr>,
    pub tx_type: Option<E::Fr>,
}
impl<E: JubjubEngine> DepositWitness<E> {
    pub fn get_pubdata(&self) -> Vec<bool> {
        let mut pubdata_bits = vec![];
        append_be_fixed_width(
            &mut pubdata_bits,
            &self.tx_type.unwrap(),
            *franklin_constants::TX_TYPE_BIT_WIDTH,
        );

        append_be_fixed_width(
            &mut pubdata_bits,
            &self.before.address.unwrap(),
            franklin_constants::ACCOUNT_TREE_DEPTH,
        );
        append_be_fixed_width(
            &mut pubdata_bits,
            &self.before.token.unwrap(),
            *franklin_constants::TOKEN_EXT_BIT_WIDTH,
        );
        append_be_fixed_width(
            &mut pubdata_bits,
            &self.args.amount.unwrap(),
            franklin_constants::AMOUNT_MANTISSA_BIT_WIDTH
                + franklin_constants::AMOUNT_EXPONENT_BIT_WIDTH,
        );

        append_be_fixed_width(
            &mut pubdata_bits,
            &self.args.fee.unwrap(),
            franklin_constants::FEE_MANTISSA_BIT_WIDTH + franklin_constants::FEE_EXPONENT_BIT_WIDTH,
        );

        let mut new_pubkey_bits = vec![];
        append_le_fixed_width(
            &mut new_pubkey_bits,
            &self.args.new_pub_y.unwrap(),
            franklin_constants::FR_BIT_WIDTH - 1,
        );
        append_le_fixed_width(&mut new_pubkey_bits, &self.args.new_pub_x.unwrap(), 1);

        let phasher = PedersenHasher::<Bn256>::default();
        let new_pubkey_hash = phasher.hash_bits(new_pubkey_bits);

        append_be_fixed_width(
            &mut pubdata_bits,
            &new_pubkey_hash,
            *franklin_constants::NEW_PUBKEY_HASH_WIDTH,
        );
        assert_eq!(pubdata_bits.len(), 37 * 8);
        pubdata_bits.resize(40 * 8, false);
        pubdata_bits
    }
}
pub fn apply_deposit(
    tree: &mut CircuitAccountTree,
    deposit: &DepositData,
) -> DepositWitness<Bn256> {
    //preparing data and base witness
    let before_root = tree.root_hash();
    println!("Initial root = {}", before_root);
    let (audit_path_before, audit_balance_path_before) =
        get_audits(tree, deposit.account_address, deposit.token);

    let capacity = tree.capacity();
    assert_eq!(capacity, 1 << franklin_constants::ACCOUNT_TREE_DEPTH);
    let account_address_fe = Fr::from_str(&deposit.account_address.to_string()).unwrap();
    let token_fe = Fr::from_str(&deposit.token.to_string()).unwrap();
    let amount_as_field_element = Fr::from_str(&deposit.amount.to_string()).unwrap();

    let amount_bits = convert_to_float(
        deposit.amount,
        *franklin_constants::AMOUNT_EXPONENT_BIT_WIDTH,
        *franklin_constants::AMOUNT_MANTISSA_BIT_WIDTH,
        10,
    )
    .unwrap();

    let amount_encoded: Fr = le_bit_vector_into_field_element(&amount_bits);

    let fee_as_field_element = Fr::from_str(&deposit.fee.to_string()).unwrap();

    let fee_bits = convert_to_float(
        deposit.fee,
        *franklin_constants::FEE_EXPONENT_BIT_WIDTH,
        *franklin_constants::FEE_MANTISSA_BIT_WIDTH,
        10,
    )
    .unwrap();

    let fee_encoded: Fr = le_bit_vector_into_field_element(&fee_bits);

    //calculate a and b
    let a = amount_as_field_element.clone();
    let b = fee_as_field_element.clone();

    //applying deposit
    let (account_witness_before, account_witness_after, balance_before, balance_after) =
        apply_leaf_operation(
            tree,
            deposit.account_address,
            deposit.token,
            |acc| {
                assert!(
                    (acc.pub_x == deposit.new_pub_x && acc.pub_y == deposit.new_pub_y)
                        || (acc.pub_y == Fr::zero() && acc.pub_x == Fr::zero())
                );
                acc.pub_x = deposit.new_pub_x;
                acc.pub_y = deposit.new_pub_y;
            },
            |bal| bal.value.add_assign(&amount_as_field_element),
        );

    let after_root = tree.root_hash();
    println!("After root = {}", after_root);
    let (audit_path_after, audit_balance_path_after) =
        get_audits(tree, deposit.account_address, deposit.token);

    DepositWitness {
        before: OperationBranch {
            address: Some(account_address_fe),
            token: Some(token_fe),
            witness: OperationBranchWitness {
                account_witness: account_witness_before,
                account_path: audit_path_before,
                balance_value: Some(balance_before),
                balance_subtree_path: audit_balance_path_before,
            },
        },
        after: OperationBranch {
            address: Some(account_address_fe),
            token: Some(token_fe),
            witness: OperationBranchWitness {
                account_witness: account_witness_after,
                account_path: audit_path_after,
                balance_value: Some(balance_after),
                balance_subtree_path: audit_balance_path_after,
            },
        },
        args: OperationArguments {
            amount: Some(amount_encoded),
            fee: Some(fee_encoded),
            a: Some(a),
            b: Some(b),
            new_pub_x: Some(deposit.new_pub_x),
            new_pub_y: Some(deposit.new_pub_y),
        },
        before_root: Some(before_root),
        after_root: Some(after_root),
        tx_type: Some(Fr::from_str("1").unwrap()),
    }
}

#[test]
fn test_deposit_franklin_in_empty_leaf() {
    use super::utils::public_data_commitment;
    
    use crate::circuit::FranklinCircuit;
    use crate::operation::*;
    use crate::utils::*;
    use bellman::Circuit;
    
    
    use ff::{BitIterator, Field, PrimeField};
    use franklin_crypto::alt_babyjubjub::AltJubjubBn256;
    
    use franklin_crypto::circuit::test::*;
    use franklin_crypto::eddsa::{PrivateKey, PublicKey};
    use franklin_crypto::jubjub::FixedGenerators;
    use franklinmodels::circuit::account::{
        Balance, CircuitAccount, CircuitAccountTree, CircuitBalanceTree,
    };
    use franklinmodels::params as franklin_constants;
    
    
    use pairing::bn256::*;
    use rand::{Rng, SeedableRng, XorShiftRng};

    let params = &AltJubjubBn256::new();
    let p_g = FixedGenerators::SpendingKeyGenerator;
    let validator_address = Fr::from_str("7").unwrap();
    let block_number = Fr::from_str("1").unwrap();
    let rng = &mut XorShiftRng::from_seed([0x3dbe_6258, 0x8d31_3d76, 0x3237_db17, 0xe5bc_0654]);
    // let phasher = PedersenHasher::<Bn256>::default();

    let mut tree: CircuitAccountTree =
        CircuitAccountTree::new(franklin_constants::ACCOUNT_TREE_DEPTH as u32);

    let sender_sk = PrivateKey::<Bn256>(rng.gen());
    let sender_pk = PublicKey::from_private(&sender_sk, p_g, params);
    let (sender_x, sender_y) = sender_pk.0.into_xy();
    println!("x = {}, y = {}", sender_x, sender_y);

    // give some funds to sender and make zero balance for recipient

    let mut account_address: u32 = rng.gen();
    account_address %= tree.capacity();
    let amount: u128 = 500;
    let fee: u128 = 0;
    let token: u32 = 2;
    let deposit_witness = apply_deposit(
        &mut tree,
        &DepositData {
            amount: amount,
            fee: fee,
            token: token,
            account_address: account_address,
            new_pub_x: sender_x,
            new_pub_y: sender_y,
        },
    );
    let pubdata_chunks: Vec<_> = deposit_witness
        .get_pubdata()
        .chunks(64)
        .map(|x| le_bit_vector_into_field_element(&x.to_vec()))
        .collect();

    let sig_msg = Fr::from_str("2").unwrap(); //dummy sig msg cause skipped on deposit proof
    let mut sig_bits: Vec<bool> = BitIterator::new(sig_msg.into_repr()).collect();
    sig_bits.reverse();
    sig_bits.truncate(80);

    // println!(" capacity {}",<Bn256 as JubjubEngine>::Fs::Capacity);
    let signature = sign(&sig_bits, &sender_sk, p_g, params, rng);
    //assert!(tree.verify_proof(sender_leaf_number, sender_leaf.clone(), tree.merkle_path(sender_leaf_number)));

    let operation_zero = Operation {
        new_root: deposit_witness.after_root.clone(),
        tx_type: deposit_witness.tx_type,
        chunk: Some(Fr::from_str("0").unwrap()),
        pubdata_chunk: Some(pubdata_chunks[0]),
        sig_msg: Some(sig_msg.clone()),
        signature: signature.clone(),
        signer_pub_key_x: deposit_witness.args.new_pub_x,
        signer_pub_key_y: deposit_witness.args.new_pub_y,
        args: deposit_witness.args.clone(),
        lhs: deposit_witness.before.clone(),
        rhs: deposit_witness.before.clone(),
    };

    println!("pubdata_chunk number {} is {}", 1, pubdata_chunks[1]);
    let operation_one = Operation {
        new_root: deposit_witness.after_root.clone(),
        tx_type: deposit_witness.tx_type,
        chunk: Some(Fr::from_str("1").unwrap()),
        pubdata_chunk: Some(pubdata_chunks[1]),
        sig_msg: Some(sig_msg.clone()),
        signature: signature.clone(),
        signer_pub_key_x: deposit_witness.args.new_pub_x,
        signer_pub_key_y: deposit_witness.args.new_pub_y,
        args: deposit_witness.args.clone(),
        lhs: deposit_witness.after.clone(),
        rhs: deposit_witness.after.clone(),
    };

    println!("pubdata_chunk number {} is {}", 2, pubdata_chunks[2]);
    let operation_two = Operation {
        new_root: deposit_witness.after_root.clone(),
        tx_type: deposit_witness.tx_type,
        chunk: Some(Fr::from_str("2").unwrap()),
        pubdata_chunk: Some(pubdata_chunks[2]),
        sig_msg: Some(sig_msg.clone()),
        signature: signature.clone(),
        signer_pub_key_x: deposit_witness.args.new_pub_x,
        signer_pub_key_y: deposit_witness.args.new_pub_y,
        args: deposit_witness.args.clone(),
        lhs: deposit_witness.after.clone(),
        rhs: deposit_witness.after.clone(),
    };

    let operation_three = Operation {
        new_root: deposit_witness.after_root.clone(),
        tx_type: deposit_witness.tx_type,
        chunk: Some(Fr::from_str("3").unwrap()),
        pubdata_chunk: Some(pubdata_chunks[3]),
        sig_msg: Some(sig_msg.clone()),
        signature: signature.clone(),
        signer_pub_key_x: deposit_witness.args.new_pub_x,
        signer_pub_key_y: deposit_witness.args.new_pub_y,
        args: deposit_witness.args.clone(),
        lhs: deposit_witness.after.clone(),
        rhs: deposit_witness.after.clone(),
    };
    let operation_four = Operation {
        new_root: deposit_witness.after_root.clone(),
        tx_type: deposit_witness.tx_type,
        chunk: Some(Fr::from_str("4").unwrap()),
        pubdata_chunk: Some(pubdata_chunks[4]),
        sig_msg: Some(sig_msg.clone()),
        signature: signature.clone(),
        signer_pub_key_x: deposit_witness.args.new_pub_x,
        signer_pub_key_y: deposit_witness.args.new_pub_y,
        args: deposit_witness.args.clone(),
        lhs: deposit_witness.after.clone(),
        rhs: deposit_witness.after.clone(),
    };

    let public_data_commitment = public_data_commitment::<Bn256>(
        &deposit_witness.get_pubdata(),
        deposit_witness.before_root,
        deposit_witness.after_root,
        Some(validator_address),
        Some(block_number),
    );
    {
        let mut cs = TestConstraintSystem::<Bn256>::new();

        let instance = FranklinCircuit {
            params,
            old_root: deposit_witness.before_root,
            new_root: deposit_witness.after_root,
            operations: vec![
                operation_zero,
                operation_one,
                operation_two,
                operation_three,
                operation_four,
            ],
            pub_data_commitment: Some(public_data_commitment),
            block_number: Some(Fr::one()),
            validator_address: Some(validator_address),
        };

        instance.synthesize(&mut cs).unwrap();

        println!("{}", cs.find_unconstrained());

        println!("{}", cs.num_constraints());

        let err = cs.which_is_unsatisfied();
        if err.is_some() {
            panic!("ERROR satisfying in {}", err.unwrap());
        }
    }
}
