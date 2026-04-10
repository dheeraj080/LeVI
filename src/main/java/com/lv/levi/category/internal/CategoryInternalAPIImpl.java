package com.lv.levi.category.internal;

import com.lv.levi.category.CategoryInternalAPI;
import com.lv.levi.category.CategoryMetadata;
import com.lv.levi.category.repository.CategoryRepository;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;

import java.util.UUID;

@Service
@RequiredArgsConstructor
class CategoryInternalAPIImpl implements CategoryInternalAPI {

    private final CategoryRepository categoryRepository;

    @Override
    public boolean existsByIdAndUserId(UUID categoryId, UUID userId) {
        // Ensure your CategoryRepository has this method name
        return categoryRepository.existsByIdAndUserId(categoryId, userId);
    }

    @Override
    public CategoryMetadata getCategoryMetadata(UUID categoryId) {
        return categoryRepository.findById(categoryId)
                .map(cat -> new CategoryMetadata(cat.getName(), cat.getIcon()))
                .orElse(new CategoryMetadata("Uncategorized", "folder"));
    }
}