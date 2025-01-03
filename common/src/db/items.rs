use crate::{
    db::DbItem,
    tx::{
        compute::{Result, ResultReference},
        Tx, TxSequence, TxTimestamp,
    },
    tx_event::TxEvent,
};
use alloy_rlp::encode;
use sha3::{Digest, Keccak256};

impl DbItem for TxEvent {
    fn get_key(&self) -> Vec<u8> {
        let mut hasher = Keccak256::new();
        hasher.update(self.block_number().to_be_bytes());
        hasher.update(encode(self.proof()));
        let result = hasher.finalize();
        result.to_vec()
    }

    fn get_cf() -> String {
        "tx_event".to_string()
    }

    fn get_prefix(&self) -> String {
        "tx_event".to_string()
    }
}

impl DbItem for Result {
    fn get_key(&self) -> Vec<u8> {
        self.get_seq_number().to_be_bytes().to_vec()
    }

    fn get_cf() -> String {
        "metadata".to_string()
    }

    fn get_prefix(&self) -> String {
        "result".to_string()
    }
}

impl DbItem for ResultReference {
    fn get_key(&self) -> Vec<u8> {
        self.compute_request_tx_hash().to_bytes()
    }

    fn get_prefix(&self) -> String {
        String::new()
    }

    fn get_cf() -> String {
        "result_reference".to_string()
    }
}

impl DbItem for Tx {
    fn get_key(&self) -> Vec<u8> {
        self.hash().to_bytes()
    }

    fn get_cf() -> String {
        "tx".to_string()
    }

    fn get_prefix(&self) -> String {
        self.body().prefix().to_string()
    }
}

impl DbItem for TxSequence {
    fn get_key(&self) -> Vec<u8> {
        self.seq_number().to_be_bytes().to_vec()
    }

    fn get_cf() -> String {
        "tx_sequence".to_string()
    }

    fn get_prefix(&self) -> String {
        String::new()
    }
}

impl DbItem for TxTimestamp {
    fn get_key(&self) -> Vec<u8> {
        self.tx_hash().to_bytes()
    }

    fn get_cf() -> String {
        "tx_timestamp".to_string()
    }

    fn get_prefix(&self) -> String {
        String::new()
    }
}

#[cfg(test)]
mod test {
    use super::TxEvent;
    use crate::{
        db::DbItem,
        tx::{self, compute, Tx},
    };
    use alloy_rlp::encode;

    #[test]
    fn test_tx_event_db_item() {
        let tx_event = TxEvent::default_with_data(encode(Tx::default_with(
            tx::Body::ComputeRequest(compute::Request::default()),
        )));

        let key = tx_event.get_key();
        assert_eq!(
            hex::encode(key),
            "b486c7ce0b8114c45571048c16ebc834f680ca021612d347a6560aa001bdfe97"
        );
    }
}
