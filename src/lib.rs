use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    env, 
    near_bindgen,
    log,
    collections:: {UnorderedMap, Vector},
    AccountId,
    Balance,
    Promise,
};

near_sdk::setup_alloc!();

//
const TICKET_PRICE : u128 = 2_000_000_000_000_000_000_000_000; //2 NEAR
const CHARITY_RATE : u128 =   400_000_000_000_000_000_000_000; //20%

//CharityFund
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct CharityFund {
    charity_id : AccountId,
    name : String,
    voted_count : u32,
}
//
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, PartialEq, Clone)]
#[serde(crate = "near_sdk::serde")]
pub enum LotStatus {
    Open,
    Paused,
    Done,
}

impl LotStatus {
    pub fn is_done(&self) -> bool {
        self == &LotStatus::Done
    }

    pub fn is_open(&self) -> bool {
        self == &LotStatus::Open
    }

    pub fn is_paused(&self) -> bool {
        self == &LotStatus::Paused
    }
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct CharityLot {
    charity_map : UnorderedMap<AccountId, CharityFund>,
    paticipants : Vector<AccountId>,    //who joins the lottery
    lottery_pot : u128,                 
    charity_pot : u128,
    ticket_price : u128,
    charity_rate : u128,         //default 0.2
    council_id  : AccountId,    //present for DAO in the future
    lot_status  : LotStatus,
    highest_voted_charity : AccountId,
    lot_winner : AccountId,
    //open_period,
    //open_period_end
}

impl Default for CharityLot{
    fn default() -> Self{
        env::panic(b"CharityLot should be initialized before usage")
    }
}

#[near_bindgen]
impl CharityLot {
    #[init]
    pub fn new(council_id : AccountId) -> Self{
        assert!(
            env::is_valid_account_id(council_id.as_bytes()),
            "Council's account id is invalid"
        );
        assert!(!env::state_exists(), "Already initialized");

        Self{
            charity_map : UnorderedMap::new(b"charity_map".to_vec()),
            paticipants : Vector::new(b"paticipants".to_vec()),
            lottery_pot : 0,
            charity_pot : 0,
            ticket_price : TICKET_PRICE,
            charity_rate : CHARITY_RATE,
            council_id : council_id,
            lot_status : LotStatus::Done,
            highest_voted_charity : AccountId::new(),
            lot_winner : AccountId::new(),
        }
    }

    //just work with: only_council + status == Done
    pub fn set_charity_fund(&mut self, acc_id : AccountId, name : String){
        self.only_council();
        assert!(self.lot_status.is_done(), "status is not done. Can't set_charity_fund");
        assert!(env::is_valid_account_id(acc_id.as_bytes()), "Charity account id is invalid");
        let charity_fund = CharityFund {
            charity_id : acc_id.clone(),
            name : name,
            voted_count : 0,
        };

        self.charity_map.insert(&acc_id, &charity_fund);
    }

    
    //council_id not change
    pub fn reset_state(&mut self){
        self.only_council();
        assert!(self.lot_status.is_done(), "status is not done. Can't reset_state");
        self.charity_map = UnorderedMap::new(b"charity_map".to_vec());
        self.paticipants = Vector::new(b"paticipants".to_vec());
        self.lottery_pot = 0;
        self.charity_pot = 0;
        self.ticket_price = TICKET_PRICE;   //can be set by council
        self.charity_rate = CHARITY_RATE;   //can be set by council
        self.lot_status = LotStatus::Done;

        log!("state is reset!");
    }

    pub fn update_status(&mut self, new_status : LotStatus){
        assert!(new_status != self.lot_status, "The same status! Nothing changed");
        self.only_council();

        self.lot_status = new_status;
        log!("lot_status is updated!");

        if self.lot_status.is_done() {
            self.lot_winner = self.random_winner();
            self.highest_voted_charity = self.get_charity_win();
            //
            self.transfer_winners();
        }
    }

    

    //  buy a lottery ticket
    //  the amount of (1 - charity_rate)*ticket_price --> lottery_pot
    //  the remaining --> charity_pot
    //  Note: attached_deposit can be larger ticket_price
    #[payable]
    pub fn buy_ticket(&mut self, charity_acc_id : AccountId){
        assert!(self.lot_status.is_open(), "lot-status is not open! Cannot buy ticket now!");
        assert!(env::attached_deposit() >= self.ticket_price, "Not enough deposit");

        let charity_option = self.charity_map.get(&charity_acc_id);
        assert!(charity_option.is_some(), "There is no charity fund corresponding");

        //TODO: check overflow
        let deposit = env::attached_deposit();
        let part_lot_pot = self.ticket_price - self.charity_rate;
        self.lottery_pot += part_lot_pot;
        self.charity_pot += deposit - part_lot_pot;

        //update voted_count
        let mut charity_fund = charity_option.unwrap();
        charity_fund.voted_count += 1;
        self.charity_map.insert(&charity_acc_id, &charity_fund);

        //update participants
        let participant = env::predecessor_account_id();
        self.paticipants.push(&participant);
        
        //log for testing
        log!("buy ticket successfully! deposit = {}, part-lot = {}, total-charity = {}", deposit, part_lot_pot, self.charity_pot);
        log!("len participants {}, index0 {} indexlen-1 {}", self.paticipants.len(), self.paticipants.get(0).unwrap(), self.paticipants.get(self.paticipants.len()-1).unwrap());
    }

    // pub fn transfer_winners(&mut self, to_charity : AccountId, to_winner : AccountId){
    //     self.only_council();

    //     Promise::new(to_charity).transfer(self.charity_pot);
    //     Promise::new(to_winner).transfer(self.lottery_pot);
    // }


    fn transfer_winners(&mut self){
        log!("lot_winner {} , highest_voted_charity {} ", self.lot_winner, self.highest_voted_charity );

        self.only_council();
        // let to_charity = &self.highest_voted_charity;
        // let to_winner = &self.lot_winner;

        let to_charity = self.highest_voted_charity.clone();
        let to_winner = self.lot_winner.clone();

        Promise::new(to_charity).transfer(self.charity_pot);
        Promise::new(to_winner).transfer(self.lottery_pot);
    }

    
    //modifiers
    pub fn only_council(&self) {
        let signer = env::signer_account_id();
        if signer != self.council_id {
            env::panic(b"you dont have permission !")
        }
    }
    
    //view methods
    pub fn get_council(&self) -> AccountId {
        self.council_id.clone()
    }

    pub fn get_charity_funds(&self) -> Vec<CharityFund> { 
        let values = self.charity_map.values_as_vector();
        (0..values.len())
            .map(|index| values.get(index).unwrap())
            .collect()
    }

    pub fn get_participants(&self) -> Vec<AccountId> {
        (0..self.paticipants.len())
            .map(|index| self.paticipants.get(index).unwrap())
            .collect()
    }

    pub fn get_lot_status(&self) -> LotStatus {
        match self.lot_status {
            LotStatus::Open => LotStatus::Open,
            LotStatus::Paused => LotStatus::Paused,
            _ => LotStatus::Done,
        }
    }

    fn get_charity_win(&self) -> AccountId{
        let keys = self.charity_map.keys_as_vector();
        let mut max_count : u32 = 0;
        let mut at_index : u64 = 1_000_000_000_000; //FIXME: just test, MUST fix

        for i in 0..keys.len() {
            match keys.get(i){
                Some(v) => {
                    let charity_fund = self.charity_map.get(&v).unwrap();
                    if charity_fund.voted_count >= max_count {  //FIXME: in case equal ???
                        max_count = charity_fund.voted_count;
                        at_index = i;
                    }
                },
                _ => {
                    //DO NOTHING
                }
            }
        }

        if at_index != 1_000_000_000_000 {
            match keys.get(at_index) {
                Some(v) => v,
                _ => env::panic(b"THERE IS NOTHING !")
            }
        }else{
            env::panic(b"THERE IS NOTHING !")
        }

    }

    fn random_winner(&self) -> AccountId{
        //TODO: improve this code 
        let max = self.paticipants.len() as u8;
        let v : Vec<u8> = env::random_seed();
        let number = v[0];
        let index = number % max;

        log!("random ... winner at index {} ", index);
        self.paticipants.get(index as u64).unwrap()
    }
}

