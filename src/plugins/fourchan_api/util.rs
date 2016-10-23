pub fn strip_schema(val: &str) -> Option<&str> {
	const VALID_SCHEMAS: &'static [&'static str] = &["http://", "https://"];

	for schema in VALID_SCHEMAS.iter() {
		if val.starts_with(schema) {
			return Some(&val[schema.len()..]);
		}
	}
	None
}

pub fn is_image_host(val: &str) -> bool {
	const VALID_IMAGE_HOSTS: &'static [&'static str] = &["i.4cdn.org", "is.4chan.org"];

	for ihost in VALID_IMAGE_HOSTS.iter() {
		if val == *ihost {
			return true;
		}
	}
	false
}

