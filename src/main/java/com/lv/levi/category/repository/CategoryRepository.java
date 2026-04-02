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

    // 1. Use the 'User' object directly - much safer
    List<Category> findByUserAndDeletedFalse(User user);

    // 2. Security check: does this ID belong to this User?
    Optional<Category> findByIdAndUserAndDeletedFalse(UUID id, User user);

    // 3. Unique check per user
    boolean existsByNameAndUserAndDeletedFalse(String name, User user);

    // 4. Filter by type (Income/Expense) for a specific user
    List<Category> findByUserAndTypeAndDeletedFalse(User user, CategoryType type);

    // This is the one we added for the Transaction security check
    Optional<Category> findByIdAndUser(UUID id, User user);
}