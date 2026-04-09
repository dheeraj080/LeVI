package com.lv.levi.category;

import com.lv.levi.auth.entity.User;
import com.lv.levi.category.dto.CategoryDTO;
import com.lv.levi.category.entity.Category;
import com.lv.levi.category.entity.CategoryType;
import com.lv.levi.category.repository.CategoryRepository;
import com.lv.levi.category.service.CategoryService;
import jakarta.persistence.EntityNotFoundException;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.util.Optional;
import java.util.UUID;

import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class CategoryServiceTest {

    @Mock
    private CategoryRepository categoryRepository;

    @InjectMocks
    private CategoryService categoryService;

    private User testUser;
    private CategoryDTO categoryDto;

    @BeforeEach
    void setUp() {
        testUser = User.builder().id(UUID.randomUUID()).email("test@lv.com").build();
        categoryDto = CategoryDTO.builder()
                .name("Food")
                .type(CategoryType.DEBIT)
                .icon("fast-food")
                .build();
    }

    @Test
    void createCategory_Success() {
        when(categoryRepository.existsByNameAndUserAndDeletedFalse(anyString(), any())).thenReturn(false);
        when(categoryRepository.save(any(Category.class))).thenAnswer(i -> i.getArguments()[0]);

        CategoryDTO result = categoryService.createCategory(categoryDto, testUser);

        assertNotNull(result);
        assertEquals("Food", result.getName());
        verify(categoryRepository).save(any(Category.class));
    }

    @Test
    void deleteCategory_ShouldThrowException_WhenNotFound() {
        UUID id = UUID.randomUUID();
        when(categoryRepository.findByIdAndUserAndDeletedFalse(id, testUser)).thenReturn(Optional.empty());

        assertThrows(EntityNotFoundException.class, () -> categoryService.deleteCategory(id, testUser));
    }
}