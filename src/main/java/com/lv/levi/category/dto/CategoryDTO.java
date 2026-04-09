package com.lv.levi.category.dto;

import com.lv.levi.category.entity.CategoryType;
import lombok.AllArgsConstructor;
import lombok.Builder;
import lombok.Data;
import lombok.NoArgsConstructor;

import java.time.Instant;
import java.util.UUID;

@Data
@NoArgsConstructor
@AllArgsConstructor
@Builder
public class CategoryDTO {

    private UUID id;

    // Changed to UUID to match the User entity's ID type
    private UUID profileId;

    private String name;
    private String icon;

    private CategoryType type;

    // Changed to Instant to match Category entity
    private Instant createdAt;
    private Instant updatedAt;
}
