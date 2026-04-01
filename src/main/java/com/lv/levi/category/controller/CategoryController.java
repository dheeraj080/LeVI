package com.lv.levi.category.controller;

import com.lv.levi.auth.entity.User;
import com.lv.levi.category.dto.CategoryDto;
import com.lv.levi.category.service.CategoryService;
import jakarta.validation.Valid;
import lombok.RequiredArgsConstructor;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.security.core.annotation.AuthenticationPrincipal;
import org.springframework.validation.annotation.Validated;
import org.springframework.web.bind.annotation.*;

import java.util.List;
import java.util.UUID;

@RestController
@RequestMapping("/api/v1/categories")
@RequiredArgsConstructor
@Validated // Add this for method-level validation if needed
public class CategoryController {

    private final CategoryService categoryService;

    @GetMapping
    public ResponseEntity<List<CategoryDto>> getAllCategories(@AuthenticationPrincipal User user) {
        // Consider pagination if a user might have hundreds of categories
        return ResponseEntity.ok(categoryService.getAllCategoriesForUser(user.getId()));
    }

    @PostMapping
    public ResponseEntity<CategoryDto> createCategory(
            @Valid @RequestBody CategoryDto categoryDto, // Added @Valid
            @AuthenticationPrincipal User user) {
        return new ResponseEntity<>(categoryService.createCategory(categoryDto, user), HttpStatus.CREATED);
    }

    @PutMapping("/{id}")
    public ResponseEntity<CategoryDto> updateCategory(
            @PathVariable UUID id,
            @Valid @RequestBody CategoryDto categoryDto, // Added @Valid
            @AuthenticationPrincipal User user) {
        return ResponseEntity.ok(categoryService.updateCategory(id, categoryDto, user.getId()));
    }

    @DeleteMapping("/{id}")
    public ResponseEntity<Void> deleteCategory(
            @PathVariable UUID id,
            @AuthenticationPrincipal User user) {
        categoryService.deleteCategory(id, user.getId());
        return ResponseEntity.noContent().build();
    }
}