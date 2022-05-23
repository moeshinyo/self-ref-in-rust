use crate::{
    error::Result,
    loading::{Function, Library},
};
use std::ffi::{c_void, CString};

pub struct BundleFactory {
    library: Library,
}
pub struct AuthService<'a> {
    func_login: Function<&'a Library, extern "C" fn(*const c_void) -> i32>,
    func_logout: Function<&'a Library, extern "C" fn() -> i32>,
}

impl BundleFactory {
    pub fn new() -> Result<BundleFactory> {
        Ok(Self {
            library: Library::open("service.dll")?,
        })
    }
    pub fn get_auth_service(&self) -> Result<AuthService> {
        Ok(AuthService {
            func_login: Function:: from_ref(&self.library, "login")?,
            func_logout: Function:: from_ref(&self.library, "logout")?,
        })
    }
}

impl<'a> AuthService<'a> {
    pub fn login(&self, token: impl Into<Vec<u8>>) -> Result<i32> {
        let token = CString::new(token)?;
        
        Ok(unsafe { self.func_login.get_raw_fn()(token.as_ptr() as *const c_void) })
    }

    pub fn logout(&self) -> i32 {
        unsafe { self.func_logout.get_raw_fn()() }
    }
}

#[cfg(test)]
mod test {
    use super::{BundleFactory, AuthService};
    
    #[test]
    fn it_works() {
        let factory: BundleFactory = BundleFactory::new().unwrap();
        let service: AuthService = factory.get_auth_service().unwrap();

        assert_eq!(service.login("").unwrap(), 0i32);
        assert_eq!(service.logout(), 0i32);
    }
}