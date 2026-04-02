package com.lv.levi.transaction.service;

import com.lv.levi.auth.entity.User;
import com.lv.levi.auth.exceptions.ResourceNotFoundException;
import com.lv.levi.category.entity.Category;
import com.lv.levi.category.repository.CategoryRepository;
import com.lv.levi.transaction.dto.TransactionDTO;
import com.lv.levi.transaction.entity.Transaction;
import com.lv.levi.transaction.entity.TransactionType;
import com.lv.levi.transaction.repository.TransactionRepository;
import lombok.RequiredArgsConstructor;
import org.modelmapper.ModelMapper;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.List;
import java.util.UUID;
import java.util.stream.Collectors;

@Service
@RequiredArgsConstructor
@Transactional(readOnly = true) // Default to read-only for performance
public class TransactionService {

    private final TransactionRepository transactionRepository;
    private final CategoryRepository categoryRepository;
    private final ModelMapper modelMapper;

    /**
     * Get all transactions for a user, newest first.
     */
    public List<TransactionDTO> getUserTransactions(User user) {
        return transactionRepository.findAllByUserOrderByTransactionDateDesc(user)
                .stream()
                .map(this::convertToDto)
                .collect(Collectors.toList());
    }

    /**
     * Get transactions within a specific date range.
     */
    public List<TransactionDTO> getTransactionsByPeriod(User user, LocalDate start, LocalDate end) {
        return transactionRepository.findAllByUserAndTransactionDateBetweenOrderByTransactionDateDesc(user, start, end)
                .stream()
                .map(this::convertToDto)
                .collect(Collectors.toList());
    }

    /**
     * Calculate total expenses (DEBIT) for a user.
     */
    public BigDecimal getTotalExpense(User user) {
        // This now calls the updated repository method without the 'deleted' check
        BigDecimal total = transactionRepository.sumTotalByType(user, TransactionType.DEBIT);
        return total != null ? total : BigDecimal.ZERO;
    }

    /**
     * Calculate expenses for a specific category.
     */
    public BigDecimal getExpenseByCategory(User user, UUID categoryId) {
        BigDecimal total = transactionRepository.sumByCategoryAndType(user, categoryId, TransactionType.DEBIT);
        return total != null ? total : BigDecimal.ZERO;
    }

    /**
     * Create a new transaction (Write operation)
     */
    @Transactional
    public TransactionDTO createTransaction(TransactionDTO dto, User user) {
        // SECURITY FIX: Use the new scoped repository method
        Category category = categoryRepository.findByIdAndUser(dto.getCategoryId(), user)
                .orElseThrow(() -> new ResourceNotFoundException(
                        "Category not found or you do not have permission to use it."
                ));

        Transaction transaction = Transaction.builder()
                .title(dto.getTitle())
                .description(dto.getDescription())
                .amount(dto.getAmount())
                .type(dto.getType())
                .transactionDate(dto.getTransactionDate())
                .category(category) // Now verified as 'owned'
                .user(user)
                .build();

        Transaction saved = transactionRepository.save(transaction);
        return convertToDto(saved);
    }

    private TransactionDTO convertToDto(Transaction transaction) {
        TransactionDTO dto = modelMapper.map(transaction, TransactionDTO.class);
        // Ensure IDs and UI helpers are set correctly
        dto.setCategoryId(transaction.getCategory().getId());
        dto.setCategoryName(transaction.getCategory().getName());
        dto.setCategoryIcon(transaction.getCategory().getIcon());
        return dto;
    }
}