use std::ops::{Deref, DerefMut};

pub struct Reservable<T> {
    in_use: bool,
    obj: T,
}

impl<T> Drop for Reservable<T> {
    #[inline]
    fn drop(&mut self) {
        self.in_use = false;
    }
}

impl<T> Deref for Reservable<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        return &self.obj;
    }
}

impl<T> DerefMut for Reservable<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        return &mut self.obj;
    }
}


pub struct Pool<T> {
    objects: Vec<Reservable<T>>,
    next: usize,
    init_function: Box<dyn Fn() -> T>,
}


impl<T> Pool<T> {
    #[inline]
    pub fn new<F>(capacity: usize, init_function: F) -> Pool<T>
    where
        F: Fn() -> T + 'static,
    {
        let mut pool = Pool {
            objects: Vec::with_capacity(capacity),
            next: 0,
            init_function: Box::new(init_function),
        };
        pool.objects = (0..capacity)
                .into_iter()
                .map(|i| Reservable {
                    obj: init_function(),
                    in_use: false,
                })
                .collect();
        return pool;
    }

    pub fn pull(&mut self) -> Reservable<T> {
        for i in self.next..self.objects.len() {
            if !self.objects[i].in_use {
                return self.use_value(i);
            }
        }
        for i in 0..self.next {
            if !self.objects[i].in_use {
                return self.use_value(i);
            }
        }
        let new_entry = self.objects.len();
        self.objects.push(Reservable { in_use: true, obj: (self.init_function)()});
        return self.use_value(new_entry);
    }


    fn use_value(&mut self, index:usize) -> Reservable<T>{
        self.objects[index].in_use = true;
        self.next = index + 1;
        return self.objects[index];
    }
}
