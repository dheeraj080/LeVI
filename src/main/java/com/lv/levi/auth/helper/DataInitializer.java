package com.lv.levi.auth.helper;

import com.lv.levi.auth.entity.Role;
import com.lv.levi.auth.repository.RoleRepository;
import lombok.RequiredArgsConstructor;
import org.jspecify.annotations.NonNull;
import org.springframework.boot.CommandLineRunner;
import org.springframework.stereotype.Component;

import java.util.UUID;

@Component
@RequiredArgsConstructor
public class DataInitializer implements CommandLineRunner {

    private final RoleRepository roleRepository;

    @Override
    public void run(String @NonNull ... args) {
        if (roleRepository.count() == 0) {
            roleRepository.save(new Role(UUID.randomUUID(), "ROLE_USER"));
            roleRepository.save(new Role(UUID.randomUUID(), "ROLE_ADMIN"));
            System.out.println("✅ Default roles initialized.");
        }
    }
}