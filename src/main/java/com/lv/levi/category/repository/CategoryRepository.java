package com.lv.levi.category.repository;

import com.lv.levi.category.entity.Category;
import com.lv.levi.category.entity.CategoryType;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.stereotype.Repository;

import java.util.List;
import java.util.Optional;
import java.util.UUID;

@Repository
public interface CategoryRepository extends JpaRepository<Category, UUID> {

    List<Category> findByUserIdAndDeletedFalse(UUID userId);

    Optional<Category> findByUserIdAndIdAndDeletedFalse(UUID userId, UUID id);

    boolean existsByNameAndUserIdAndDeletedFalse(String name, UUID userId);

    // FIX: Changed 'ProfileUserId' to 'UserId' to match your entity field 'user'
    List<Category> findByUserIdAndTypeAndDeletedFalse(UUID userId, CategoryType type);
}