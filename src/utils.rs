pub fn get_ptr_address<T>(ptr: *const T) -> String {
    format!("0x{:x}", ptr as usize)
}

