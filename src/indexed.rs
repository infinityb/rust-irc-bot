


struct Indexed<K, T> {
	items: HashMap<K, V>,
	indicies: HashMap<String, K>
}

impl<K: Copy + Hash + Eq, V>