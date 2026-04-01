package com.lv.levi.auth.repository;

import com.lv.levi.auth.entity.Provider;
import com.lv.levi.auth.entity.User;
import org.springframework.data.jpa.repository.JpaRepository;

import javax.swing.text.html.Option;
import java.util.Optional;
import java.util.UUID;

public interface UserRepository extends JpaRepository<User, UUID> {

    Optional<User> findByEmail(String email);

    // Check if an email is already taken during signup
    Boolean existsByEmail(String email);

    // Find a user by their name (useful for profile pages)
    Optional<User> findByName(String name);

    Optional<User> findByProviderAndProviderId(Provider provider, String providerId);

    Optional<User> findByActivationCode(String code);
}
