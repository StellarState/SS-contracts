use soroban_sdk::{Address, Env, Symbol};

pub fn initialized(env: &Env, admin: &Address) {
    let topics = (Symbol::new(env, "initialized"),);
    env.events().publish(topics, admin.clone());
}

pub fn distributed(env: &Env, token: &Address, recipient: &Address, amount: i128) {
    let topics = (
        Symbol::new(env, "distributed"),
        token.clone(),
        recipient.clone(),
    );
    env.events().publish(topics, amount);
}
