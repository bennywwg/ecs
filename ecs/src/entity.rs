use std::{cell::Cell, ops::{Deref, DerefMut}, rc::{Rc, Weak}};
use std::hash::Hash;
use std::{collections::HashSet};

use crate::element::*;

pub struct Entity {
    elements: Vec<ElementHolder>,
    self_addr: EntAddr
}

impl Entity {
    pub fn erased_elements(&mut self) -> Vec<EleAddrErased> {
        self.elements.iter_mut()
        .map(|holder| holder.make_addr_erased())
        .collect::<Vec<EleAddrErased>>()
    }
    pub fn add_element<T: Element>(&mut self, val: T) -> Result<EleAddr<T>, String> {
        if self.query_element_addr::<T>().valid() {
            return Err(format!("Element of type \"{}\" is already present", std::any::type_name::<T>()));
        }
        self.elements.push(ElementHolder::new(val, self.self_addr.clone()));
        Ok(self.query_element_addr::<T>())
    }
    pub fn remove_element<T: Element>(&mut self) -> Result<(), String> {
        if let Some(index) = self.elements.iter_mut().position(|comp| comp.get_id() == std::any::TypeId::of::<T>()) {
            self.elements.remove(index);
            Ok(())
        } else {
            Err(format!("Element \"{}\" did not exist", std::any::type_name::<T>()) as String)
        }
    }
    pub fn query_element_addr<T: Element>(&mut self) -> EleAddr<T> {
        for comp in self.elements.iter_mut() {
            if comp.get_id() == std::any::TypeId::of::<T>() {
                return comp.make_addr::<T>();
            }
        }
        EleAddr::new()
    }
    pub fn query_element<T: Element>(&mut self) -> Option<EleRef<T>> {
        self.query_element_addr().get_ref()
    }
    pub fn query_element_mut<T: Element>(&mut self) -> Option<EleRefMut<T>> {
        self.query_element_addr().get_ref_mut()
    }
    
    // TODO: Make this a non-mut function
    fn find_element_index(&mut self, addr: &EleAddrErased) -> Option<usize> {
        self.elements.iter_mut().position(|element| element.make_addr_erased().eq(addr))
    }
}

pub struct EntityHolder {
    data: *mut Entity, // must be cleaned up with a Box::from_raw
    internal: Rc<Cell<i64>>
}

impl EntityHolder {
    pub fn new() -> Self {
        Self {
            data: Box::into_raw(Box::new(Entity {
                elements: Vec::new(),
                self_addr: EntAddr::new()
            })),
            internal: Rc::new(Cell::new(0))
        }
    }
    pub fn make_addr(&self) -> EntAddr {
        let a: *mut Entity = self.data;
        let b = unsafe { &mut *a };

        EntAddr {
            data: b,
            internal: Rc::downgrade(&self.internal)
        }
    }
}

impl Drop for EntityHolder {
    fn drop(&mut self) {
        unsafe { Box::from_raw(self.data) };
        if std::thread::panicking() { return; }
        assert!(self.internal.get() >= 0, "Element Holder dropped while a mutable reference is held");
        assert!(self.internal.get() <= 0, "Element Holder dropped while immutable references are held");
    }
}

#[derive(Clone)]
pub struct EntAddr {
    data: *mut Entity,
    internal: Weak<Cell<i64>>
}

impl PartialEq for EntAddr {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

impl Eq for EntAddr { }

impl Hash for EntAddr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.data.hash(state);
    }
}

impl EntAddr {
    pub fn new() -> Self {
        Self {
            data: std::ptr::null_mut(),
            internal: Weak::new()
        }
    }
    pub fn valid(&self) -> bool {
        self.internal.strong_count() > 0
    }
    pub fn get_ref(&self) -> Option<EntRef> {
        match self.internal.upgrade() {
            Some(rc) => {
                if rc.get() >= 0 {
                    return Some(EntRef::new(
                        unsafe { &*self.data },
                        self.internal.clone()
                    ))
                }
                None
            },
            None => None
        }
    }
    pub fn get_ref_mut(&mut self) -> Option<EntRefMut> {
        match self.internal.upgrade() {
            Some(rc) => {
                if rc.get() == 0 {
                    return Some(EntRefMut::new(
                        unsafe { &mut *self.data },
                        self.internal.clone()
                    ))
                }
                None
            },
            None => None
        }
    }
}

pub struct EntRef<'a> {
    data: &'a Entity,
    internal: Weak<Cell<i64>>
}

pub struct EntRefMut<'a> {
    data: &'a mut Entity,
    internal: Weak<Cell<i64>>
}

impl<'a> EntRef<'a> {
    pub fn new(data: &'a Entity, internal: Weak<Cell<i64>>) -> Self {
        if let Some(rc) = internal.upgrade() {
            assert!(rc.get() >= 0, "Instance of Entity is already borrowed mutably");

            rc.set(rc.get() + 1);
        } else {
            panic!("Immutable Reference to Entity attempted to be created from a dead address");
        }

        Self { data, internal }
    }
}
impl<'a> EntRefMut<'a> {
    pub fn new(data: &'a mut Entity, internal: Weak<Cell<i64>>) -> Self {
        if let Some(rc) = internal.upgrade() {
            assert!(rc.get() <= 0, "Instance of Entity is already borrowed immutably");
            assert!(rc.get() >= 0, "Instance of Entity is already borrowed mutably");

            rc.set(rc.get() - 1);
        } else {
            panic!("Mutable Reference to Entity attempted to be created from a dead address");
        }

        Self { data, internal }
    }
}
impl<'a> Drop for EntRef<'a> {
    fn drop(&mut self) {
        if std::thread::panicking() { return; }
        let rc = match self.internal.upgrade() {
            Some(rc) => rc,
            None => panic!("When dropping immutable reference of Entity, the holder was already destroyed")
        };
        rc.set(rc.get() - 1);
        assert!(rc.get() >= 0, "Instance of Entity's ref count somehow dropped below zero");
    }
}
impl<'a> Drop for EntRefMut<'a> {
    fn drop(&mut self) {
        if std::thread::panicking() { return; }
        let rc = match self.internal.upgrade() {
            Some(rc) => rc,
            None => panic!("When dropping mutable reference of Entity, the holder was already destroyed")
        };
        rc.set(rc.get() + 1);
        assert!(rc.get() == 0, "Instance of Entity's ref count didn't equal zero when dropping mutable reference");
    }
}
impl<'a> Deref for EntRef<'a> {
    type Target = Entity;

    fn deref(&self) -> &Self::Target {
        self.data
    }
}
impl<'a> Deref for EntRefMut<'a> {
    type Target = Entity;

    fn deref(&self) -> &Self::Target {
        self.data
    }
}
impl<'a> DerefMut for EntRefMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}

// Manager
pub struct Manager {
    entities: Vec<EntityHolder>,
    entity_destroy_queue: HashSet<EntAddr>,
    element_destroy_queue: HashSet<EleAddrErased>
}

impl Manager {
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            entity_destroy_queue: HashSet::new(),
            element_destroy_queue: HashSet::new()
        }
    }
    // TODO: Make this a non-mut function
    fn find_ent_index(&mut self, addr: &EntAddr) -> Option<usize> {
        self.entities.iter_mut().position(|ent| ent.make_addr().eq(addr))
    }
    fn resolve(&mut self) {
        {
            let cloned_destroy_queue = self.entity_destroy_queue.clone();
            self.entity_destroy_queue.clear();
            for to_destroy in cloned_destroy_queue.iter() {
                if let Some(destroy_index) = self.find_ent_index(to_destroy) {
                    self.entities.remove(destroy_index);
                }
            }
        }

        {
            let cloned_destroy_queue = self.element_destroy_queue.clone();
            self.element_destroy_queue.clear();
            for to_destroy in cloned_destroy_queue.iter() {
                if let Some(destroy_index) = self.find_ent_index(&to_destroy.get_owner()) {
                    let mut addr = self.entities[destroy_index].make_addr();
                    let mut r = addr.get_ref_mut().unwrap();
                    let ent_raw = r.deref_mut();
                    if let Some(element_index) = ent_raw.find_element_index(to_destroy) {
                        ent_raw.elements.remove(element_index);
                    }
                }
            }
        }
    }
    pub fn update(&mut self) {
        let mut index = 0 as usize;
        while index < self.entities.len() {
            let mut ent_addr = self.entities[index].make_addr();

            {
                let mut element_index = 0 as usize;
                let mut elements = ent_addr.get_ref_mut().unwrap().erased_elements();
                while element_index < elements.len() {
                    elements[element_index].get_ref_mut().unwrap().update(self, ent_addr.clone());
                    element_index += 1;
                }
            }

            self.resolve();

            index += 1;
        }

        self.resolve();
    }
    pub fn of_type<T: Element>(&mut self) -> Vec<EleAddr<T>> {
        self.entities.iter_mut()
        .filter(|ent| ent.make_addr().get_ref_mut().unwrap().query_element_addr::<T>().valid() )
        .map(|ent| { ent.make_addr().get_ref_mut().unwrap().query_element_addr::<T>() })
        .collect()
    }
    pub fn create_entity(&mut self) -> EntAddr {
        self.entities.push(EntityHolder::new());
        let mut res = self.entities.last_mut().unwrap().make_addr();
        res.get_ref_mut().expect("Entity that was just created should exist").self_addr = res.clone();
        res
    }
    pub fn destroy_entity(&mut self, addr: EntAddr) {
        self.entity_destroy_queue.insert(addr);
    }
    pub fn destroy_element(&mut self, addr: EleAddrErased) {
        self.element_destroy_queue.insert(addr);
    }
}