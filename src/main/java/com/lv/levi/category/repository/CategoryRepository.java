package com.lv.levi.category.repository;

import com.lv.levi.auth.entity.User;
import com.lv.levi.category.entity.Category;
import com.lv.levi.category.entity.CategoryType;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.stereotype.Repository;

import java.util.List;
import java.util.Optional;
import java.util.UUID;

@Repository
public interface CategoryRepository extends JpaRepository<Category, UUID> {

    // Change parameter from 'UUID userId' to 'UUID userId'
    boolean existsByIdAndUserId(UUID id, UUID userId);

    // Update these to use UUID as well to satisfy Modulith
    List<Category> findByUserIdAndDeletedFalse(UUID userId);

    Optional<Category> findByIdAndUserIdAndDeletedFalse(UUID id, UUID userId);

    boolean existsByNameAndUserIdAndDeletedFalse(String name, UUID userId);

    List<Category> findByUserIdAndTypeAndDeletedFalse(UUID userId, CategoryType type);
}