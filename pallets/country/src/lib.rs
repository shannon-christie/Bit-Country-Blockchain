#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
	debug, decl_error, decl_event, decl_module, decl_storage,
	dispatch::DispatchResult,
	ensure,
	traits::{Get, IsType, Randomness},
	StorageMap, StorageValue,
};
use frame_system::{self as system, ensure_root, ensure_signed};
use nft;
use primitives::CountryId;
use sp_core::H256;
use sp_runtime::{
	print,
	traits::{Hash, One},
	DispatchError, ModuleId, RuntimeDebug,
};
use sp_std::vec::Vec;
use unique_asset::AssetId;

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug)]
pub struct CountryAssetData {
	pub image: Vec<u8>,
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug)]
pub struct Country<AccountId> {
	pub owner: AccountId,
	pub metadata: Vec<u8>,
	pub token_address: AccountId,
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub trait Trait: system::Trait {
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_storage! {
	trait Store for Module<T: Trait> as Country {

		pub NextCountryId get(fn next_country_id): CountryId;
		pub Countries get(fn get_country): map hasher(twox_64_concat) CountryId => Option<Country<T::AccountId>>;
		pub CountryOwner get(fn get_country_owner): double_map hasher(twox_64_concat) CountryId, hasher(twox_64_concat) T::AccountId => Option<()>;
		pub AllCountriesCount get(fn all_countries_count): u64;
		pub CountryFund get (fn get_country_treasury): map hasher(twox_64_concat) CountryId => Option<ModuleId>;
		pub FreezingCountries get (fn get_freezing_country): map hasher(twox_64_concat) CountryId => Option<()>;

		Init get(fn is_init): bool;

		Nonce get(fn nonce): u32;
	}
}

decl_event!(
	pub enum Event<T> where AccountId = <T as system::Trait>::AccountId {
		NewCountryCreated(CountryId),
		TransferredCountry(CountryId, AccountId, AccountId),
		CountryFreezed(CountryId),
		CountryDestroyed(CountryId),
		CountryUnFreezed(CountryId),
	}
);

decl_error! {
	pub enum Error for Module<T: Trait> {
		//Country Info not found
		CountryInfoNotFound,
		//Country Id not found
		CountryIdNotFound,
		//No permission
		NoPermission,
		//No available country id
		NoAvailableCountryId,
	}
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event() = default;

		#[weight = 10_000]
		fn create_country(origin, metadata: Vec<u8>) -> DispatchResult {

			let owner = ensure_signed(origin)?;

			let country_id = Self::new_country(&owner, metadata)?;
			//Static module fund, will change to dynamic with randomness
			let module_id: ModuleId = ModuleId(*b"Country!");
			CountryFund::insert(country_id, module_id);

			CountryOwner::<T>::insert(country_id, owner, ());

			let total_country_count = Self::all_countries_count();

			let new_total_country_count = total_country_count.checked_add(One::one()).ok_or("Overflow adding new count to total country")?;
			AllCountriesCount::put(new_total_country_count);
			Self::deposit_event(RawEvent::NewCountryCreated(country_id.clone()));

			Ok(())
		}

		#[weight = 100_000]
		fn transfer_country(origin, to: T::AccountId, country_id: CountryId) -> DispatchResult {

			let who = ensure_signed(origin)?;
			// Get owner of the country
			CountryOwner::<T>::try_mutate_exists(
				&country_id, &who, |country_by_owner| -> DispatchResult {
					//ensure there is record of the country owner with country id, account id and delete them
					ensure!(country_by_owner.is_some(), Error::<T>::NoPermission);

					if who == to {
						// no change needed
						return Ok(());
					}

					*country_by_owner = None;
					CountryOwner::<T>::insert(country_id.clone(),to.clone(), ());

					Countries::<T>::try_mutate_exists(
						&country_id,
						|country| -> DispatchResult{
							let mut country_record = country.as_mut().ok_or(Error::<T>::NoPermission)?;
							country_record.owner = to.clone();
							Self::deposit_event(RawEvent::TransferredCountry(country_id, who.clone(), to.clone()));

							Ok(())
						}
					)
			})	
		}

		#[weight = 10_000]
		fn freeze_country(origin, country_id: CountryId) -> DispatchResult {
			//Only Council can free a country
			ensure_root(origin)?;

			FreezingCountries::insert(country_id, ());
			Self::deposit_event(RawEvent::CountryFreezed(country_id));

			Ok(())
		}

		#[weight = 10_000]
		fn unfreeze_country(origin, country_id: CountryId) -> DispatchResult {
			//Only Council can free a country
			ensure_root(origin)?;

			FreezingCountries::try_mutate(country_id, |freeze_country| -> DispatchResult{
				ensure!(freeze_country.take().is_some(), Error::<T>::CountryInfoNotFound);

				Self::deposit_event(RawEvent::CountryUnFreezed(country_id));
				Ok(())
			})
		}

		#[weight = 10_000]
		fn destroy_country(origin, country_id: CountryId) -> DispatchResult {
			//Only Council can destroy a country
			ensure_root(origin)?;

			Countries::<T>::try_mutate(country_id, |country_info| -> DispatchResult{
				let t = country_info.take().ok_or(Error::<T>::CountryInfoNotFound)?;
				
				CountryOwner::<T>::remove(&country_id, t.owner.clone());
				Self::deposit_event(RawEvent::CountryDestroyed(country_id));

				Ok(())
			})
		}
	}
}

impl<T: Trait> Module<T> {
	/// Reads the nonce from storage, increments the stored nonce, and returns
	/// the encoded nonce to the caller.

	fn new_country(owner: &T::AccountId, metadata: Vec<u8>) -> Result<CountryId, DispatchError> {
		let country_id = NextCountryId::try_mutate(|id| -> Result<CountryId, DispatchError> {
			let current_id = *id;
			*id = id
				.checked_add(One::one())
				.ok_or(Error::<T>::NoAvailableCountryId)?;
			Ok(current_id)
		})?;

		let country_info = Country {
			owner: owner.clone(),
			token_address: Default::default(),
			metadata,
		};

		Countries::<T>::insert(country_id, country_info);

		Ok(country_id)
	}
}
