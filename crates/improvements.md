1. Update balance from ledger and ledger core, make sure to return a HashMap<Asset, Amount>
  1. After this change most likely `BalanceEntry` is no longer needed
2. Update `compute_tx_id`, it is enough to just include the debits in the preimage, since the credits can vary the ID. The idepontecy key is not part of it.
3. Calculate the compute_tx_id to check when reading if the db was tampered with.
4. Amount::new is infallible, and identical to Amount::new_unchecked
5. Asset::from_cents makes no sense, it returns a string. It should return Self. Make sure this function is used somewhere. The current body of the function could be the ToString of the Amount
6. Asset::parse_qty should return self, see if it can be merged with the previous point.
7. Amount::to_decimal_string should be ToString
8. Remove balance_prefix, and all _by_prefix and instead introduce a account search. Also plan to have accounts aliases. For instance sale/1/reciviables/[user-id] can be an alias of user/to_pay/[sale-id]
