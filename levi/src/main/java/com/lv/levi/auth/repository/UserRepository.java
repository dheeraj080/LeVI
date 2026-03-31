package com.lv.levi.auth.repository;


import com.lv.levi.auth.entity.Users;
import org.springframework.data.jpa.repository.JpaRepository;

import java.util.Optional;
import java.util.UUID;

public interface UserRepository extends JpaRepository<Users, UUID> {

    Optional<Users> findByEmail(String email);

    Optional<Users> findByActivationCode(String ActivationCode);
}
