package com.lv.levi.auth.dto;


import com.lv.levi.auth.entity.Provider;
import lombok.*;

import java.time.Instant;
import java.util.HashSet;
import java.util.Set;
import java.util.UUID;


@Data
@AllArgsConstructor
@NoArgsConstructor
@Builder
public class UserDTO {

    private UUID id; // Usually null during registration
    private String name;
    private String email;
    private String password; // Raw password from the user
    private String image;

    // Use the 'is' prefix manually or ensure it matches the JSON property
    private Boolean enabled;

    private Instant createdAt;
    private Instant updatedAt;

    private Provider provider;

    // Use RoleDTO to avoid circular references with the User Entity
    private Set<RoleDTO> roles = new HashSet<>();
}