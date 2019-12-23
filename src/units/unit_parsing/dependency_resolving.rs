use crate::units::*;
use std::collections::HashMap;

pub fn prune_units(target_unit_name: &str, unit_table: &mut HashMap<InternalId, Unit>) {}

pub fn fill_dependencies(units: &mut HashMap<InternalId, Unit>) {
    let mut name_to_id = HashMap::new();

    for (id, unit) in &*units {
        let name = unit.conf.name();
        name_to_id.insert(name, *id);
    }

    let mut required_by = Vec::new();
    let mut wanted_by: Vec<(InternalId, InternalId)> = Vec::new();
    let mut before = Vec::new();
    let mut after = Vec::new();

    for unit in (*units).values_mut() {
        let conf = &unit.conf;
        for name in &conf.wants {
            let id = name_to_id[name.as_str()];
            unit.install.wants.push(id);
            wanted_by.push((id, unit.id));
        }
        for name in &conf.requires {
            let id = name_to_id[name.as_str()];
            unit.install.requires.push(id);
            required_by.push((id, unit.id));
        }
        for name in &conf.before {
            let id = name_to_id[name.as_str()];
            unit.install.before.push(id);
            after.push((unit.id, id))
        }
        for name in &conf.after {
            let id = name_to_id[name.as_str()];
            unit.install.after.push(id);
            before.push((unit.id, id))
        }

        if let Some(conf) = &unit.install.install_config {
            for name in &conf.wanted_by {
                let id = name_to_id[name.as_str()];
                wanted_by.push((unit.id, id));
                before.push((id, unit.id));
                after.push((unit.id, id));
            }
            for name in &conf.required_by {
                let id = name_to_id[name.as_str()];
                required_by.push((unit.id, id));
                before.push((id, unit.id));
                after.push((unit.id, id));
            }
        }
    }

    for (wanted, wanting) in wanted_by {
        let unit = units.get_mut(&wanting).unwrap();
        unit.install.wants.push(wanted);
        let unit = units.get_mut(&wanted).unwrap();
        unit.install.wanted_by.push(wanting);
    }

    for (required, requiring) in required_by {
        let unit = units.get_mut(&requiring).unwrap();
        unit.install.requires.push(required);
        let unit = units.get_mut(&required).unwrap();
        unit.install.required_by.push(requiring);
    }

    for (before, after) in before {
        let unit = units.get_mut(&after).unwrap();
        unit.install.before.push(before);
    }
    for (after, before) in after {
        let unit = units.get_mut(&before).unwrap();
        unit.install.after.push(after);
    }

    for srvc in units.values_mut() {
        srvc.dedup_dependencies();
    }
}

pub fn apply_sockets_to_services(
    service_table: &mut ServiceTable,
    socket_table: &mut SocketTable,
) -> Result<(), String> {
    for sock_unit in socket_table.values_mut() {
        let mut counter = 0;

        if let UnitSpecialized::Socket(sock) = &sock_unit.specialized {
            trace!("Searching services for socket: {}", sock_unit.conf.name());
            for srvc_unit in service_table.values_mut() {
                let srvc = &mut srvc_unit.specialized;
                if let UnitSpecialized::Service(srvc) = srvc {
                    // add sockets for services with the exact same name
                    if (srvc_unit.conf.name_without_suffix()
                        == sock_unit.conf.name_without_suffix())
                        && !srvc.socket_ids.contains(&sock_unit.id)
                    {
                        trace!(
                            "add socket: {} to service: {}",
                            sock_unit.conf.name(),
                            srvc_unit.conf.name()
                        );

                        srvc.socket_ids.push(sock_unit.id);
                        srvc_unit.install.after.push(sock_unit.id);
                        sock_unit.install.before.push(srvc_unit.id);
                        counter += 1;
                    }

                    // add sockets to services that specify that the socket belongs to them
                    if let Some(srvc_conf) = &srvc.service_config {
                        if srvc_conf.sockets.contains(&sock_unit.conf.name())
                            && !srvc.socket_ids.contains(&sock_unit.id)
                        {
                            trace!(
                                "add socket: {} to service: {}",
                                sock_unit.conf.name(),
                                srvc_unit.conf.name()
                            );
                            srvc.socket_ids.push(sock_unit.id);
                            srvc_unit.install.after.push(sock_unit.id);
                            sock_unit.install.before.push(srvc_unit.id);
                            counter += 1;
                        }
                    }
                }
            }

            // add socket to the specified services
            for srvc_name in &sock.services {
                for srvc_unit in service_table.values_mut() {
                    let srvc = &mut srvc_unit.specialized;
                    if let UnitSpecialized::Service(srvc) = srvc {
                        if (*srvc_name == srvc_unit.conf.name())
                            && !srvc.socket_ids.contains(&sock_unit.id)
                        {
                            trace!(
                                "add socket: {} to service: {}",
                                sock_unit.conf.name(),
                                srvc_unit.conf.name()
                            );

                            srvc.socket_ids.push(sock_unit.id);
                            srvc_unit.install.after.push(sock_unit.id);
                            sock_unit.install.before.push(srvc_unit.id);
                            counter += 1;
                        }
                    }
                }
            }
        }
        if counter > 1 {
            return Err(format!(
                "Added socket: {} to too many services (should be at most one): {}",
                sock_unit.conf.name(),
                counter
            ));
        }
        if counter == 0 {
            warn!("Added socket: {} to no service", sock_unit.conf.name());
        }
    }

    Ok(())
}