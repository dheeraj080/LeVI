package com.lv.levi.category;

import com.lv.levi.auth.entity.User;
import com.lv.levi.auth.repository.UserRepository;
import com.lv.levi.category.entity.Category;
import com.lv.levi.category.entity.CategoryType;
import com.lv.levi.category.repository.CategoryRepository;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest;

import java.util.Optional;

import static org.junit.jupiter.api.Assertions.*;

@DataJpaTest
class CategoryRepositoryTest {

    @Autowired
    private CategoryRepository categoryRepository;

    @Autowired
    private UserRepository userRepository;

    @Test
    void shouldFindActiveCategoryByOwner() {
        // Arrange
        User user = userRepository.save(User.builder().email("owner@test.com").password("pw").build());
        Category cat = categoryRepository.save(Category.builder()
                .name("Bills")
                .user(user)
                .type(CategoryType.DEBIT)
                .deleted(false)
                .build());

        // Act
        Optional<Category> found = categoryRepository.findByIdAndUserAndDeletedFalse(cat.getId(), user);

        // Assert
        assertTrue(found.isPresent());
        assertEquals("Bills", found.get().getName());
    }
}