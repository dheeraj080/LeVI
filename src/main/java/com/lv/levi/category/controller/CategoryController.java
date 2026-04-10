package com.lv.levi.category.controller;

import com.lv.levi.auth.UserPrincipal;
import com.lv.levi.category.dto.CategoryDTO;
import com.lv.levi.category.service.CategoryService;
import jakarta.validation.Valid;
import lombok.RequiredArgsConstructor;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.security.core.annotation.AuthenticationPrincipal;
import org.springframework.web.bind.annotation.*;

import java.util.List;
import java.util.UUID;

@RestController
@RequestMapping("/api/v1/categories")
@RequiredArgsConstructor
public class CategoryController {

    private final CategoryService categoryService;

    @GetMapping
    public ResponseEntity<List<CategoryDTO>> getCategories(@AuthenticationPrincipal UserPrincipal principal) {
        return ResponseEntity.ok(categoryService.getAllCategoriesForUser(principal.id()));
    }

    // Remove: import com.lv.levi.auth.entity.User;

    @PostMapping
    public ResponseEntity<CategoryDTO> createCategory(
            @Valid @RequestBody CategoryDTO dto,
            @AuthenticationPrincipal UserPrincipal principal) { // Inject the Record

        // principal.id() gives you the UUID without needing the User entity
        return new ResponseEntity<>(categoryService.createCategory(dto, principal.id()), HttpStatus.CREATED);
    }

    @PutMapping("/{id}")
    public ResponseEntity<CategoryDTO> updateCategory(
            @PathVariable UUID id,
            @Valid @RequestBody CategoryDTO dto,
            @AuthenticationPrincipal UserPrincipal principal) {
        return ResponseEntity.ok(categoryService.updateCategory(id, dto, principal.id()));
    }

    @DeleteMapping("/{id}")
    public ResponseEntity<Void> deleteCategory(
            @PathVariable UUID id,
            @AuthenticationPrincipal UserPrincipal principal) {
        categoryService.deleteCategory(id, principal.id());
        return ResponseEntity.noContent().build();
    }
}