package com.lv.levi.category.service;

import com.lv.levi.auth.entity.User;
import com.lv.levi.category.dto.CategoryDto;
import com.lv.levi.category.entity.Category;
import com.lv.levi.category.repository.CategoryRepository;
import jakarta.persistence.EntityNotFoundException;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import com.lv.levi.category.entity.CategoryType;

import java.util.List;
import java.util.UUID;
import java.util.stream.Collectors;

@Service
@RequiredArgsConstructor
public class CategoryService {

    private final CategoryRepository categoryRepository;

    @Transactional(readOnly = true)
    public List<CategoryDto> getAllCategoriesForUser(UUID userId) {
        return categoryRepository.findByUserIdAndDeletedFalse(userId)
                .stream()
                .map(this::mapToDto)
                .collect(Collectors.toList());
    }

    @Transactional
    public CategoryDto createCategory(CategoryDto dto, User user) {
        // Prevent duplicate names for the same user
        if (categoryRepository.existsByNameAndUserIdAndDeletedFalse(dto.getName(), user.getId())) {
            throw new IllegalStateException("Category with name '" + dto.getName() + "' already exists.");
        }

        if (dto.getType() == null) {
            throw new IllegalArgumentException("Category type must be either CREDIT or DEBIT");
        }

        Category category = toEntity(dto, user);
        Category savedCategory = categoryRepository.save(category);
        return mapToDto(savedCategory);
    }

    @Transactional
    public CategoryDto updateCategory(UUID categoryId, CategoryDto dto, UUID userId) {
        // Security check: Find only if it belongs to the user and isn't deleted
        Category category = categoryRepository.findByUserIdAndIdAndDeletedFalse(userId, categoryId)
                .orElseThrow(() -> new EntityNotFoundException("Category not found or access denied"));

        // Update fields
        category.setName(dto.getName());
        category.setIcon(dto.getIcon());
        category.setType(dto.getType());

        // Save is implicit due to @Transactional dirty checking, but explicit save is fine too
        return mapToDto(categoryRepository.save(category));
    }

    @Transactional
    public void deleteCategory(UUID categoryId, UUID userId) {
        Category category = categoryRepository.findByUserIdAndIdAndDeletedFalse(userId, categoryId)
                .orElseThrow(() -> new EntityNotFoundException("Category not found or access denied"));

        category.setDeleted(true);
        categoryRepository.save(category);
    }


    public Category toEntity(CategoryDto categoryDto, User user) {
        return Category.builder()
                .name(categoryDto.getName())
                .icon(categoryDto.getIcon())
                .user(user)
                .type(categoryDto.getType())
                .deleted(false) // Ensure new categories aren't born "deleted"
                .build();
    }

    private CategoryDto mapToDto(Category entity) {
        return CategoryDto.builder()
                .id(entity.getId())
                .profileId(entity.getUser().getId())
                .name(entity.getName())
                .icon(entity.getIcon())
                .type(entity.getType()) // Entity returns CategoryType -> DTO Builder expects CategoryType
                .createdAt(entity.getCreatedAt())
                .updatedAt(entity.getUpdatedAt())
                .build();
    }
}