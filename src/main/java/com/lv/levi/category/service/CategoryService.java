package com.lv.levi.category.service;

import com.lv.levi.category.dto.CategoryDTO;
import com.lv.levi.category.entity.Category;
import com.lv.levi.category.repository.CategoryRepository;
import jakarta.persistence.EntityNotFoundException;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.util.List;
import java.util.UUID;
import java.util.stream.Collectors;

@Service
@RequiredArgsConstructor
public class CategoryService {

    private final CategoryRepository categoryRepository;

    @Transactional(readOnly = true)
    public List<CategoryDTO> getAllCategoriesForUser(UUID userId) {
        // FIX: Use userId and the updated repository method name
        return categoryRepository.findByUserIdAndDeletedFalse(userId)
                .stream()
                .map(this::mapToDto)
                .collect(Collectors.toList());
    }

    @Transactional
    public CategoryDTO createCategory(CategoryDTO dto, UUID userId) {
        // FIX: Use userId
        if (categoryRepository.existsByNameAndUserIdAndDeletedFalse(dto.getName(), userId)) {
            throw new IllegalStateException("Category '" + dto.getName() + "' already exists.");
        }

        Category category = Category.builder()
                .name(dto.getName())
                .icon(dto.getIcon())
                .type(dto.getType())
                .userId(userId) // FIX: Store the UUID directly
                .deleted(false)
                .build();

        return mapToDto(categoryRepository.save(category));
    }

    @Transactional
    public CategoryDTO updateCategory(UUID id, CategoryDTO dto, UUID userId) {
        // FIX: Use userId
        Category category = categoryRepository.findByIdAndUserIdAndDeletedFalse(id, userId)
                .orElseThrow(() -> new EntityNotFoundException("Category not found or access denied"));

        category.setName(dto.getName());
        category.setIcon(dto.getIcon());
        category.setType(dto.getType());

        return mapToDto(categoryRepository.save(category));
    }

    @Transactional
    public void deleteCategory(UUID id, UUID userId) {
        // FIX: Use userId
        Category category = categoryRepository.findByIdAndUserIdAndDeletedFalse(id, userId)
                .orElseThrow(() -> new EntityNotFoundException("Category not found or access denied"));

        category.setDeleted(true);
        categoryRepository.save(category);
    }

    private CategoryDTO mapToDto(Category entity) {
        return CategoryDTO.builder()
                .id(entity.getId())
                .name(entity.getName())
                .icon(entity.getIcon())
                .type(entity.getType())
                .createdAt(entity.getCreatedAt())
                .updatedAt(entity.getUpdatedAt())
                .build();
    }
}