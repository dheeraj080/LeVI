package com.lv.levi.transaction.service;

// 1. ALL internal imports from other modules REMOVED
import com.lv.levi.category.CategoryInternalAPI;
import com.lv.levi.transaction.dto.TransactionDTO;
import com.lv.levi.transaction.dto.TransactionSummaryDTO;
import com.lv.levi.transaction.entity.Transaction;
import com.lv.levi.transaction.entity.TransactionType;
import com.lv.levi.transaction.repository.TransactionRepository;
// This should be in a 'shared' or 'common' package to be legal
import com.lv.levi.common.exception.ResourceNotFoundException;
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
@Transactional(readOnly = true)
public class TransactionService {

    private final TransactionRepository transactionRepository;
    private final ModelMapper modelMapper;
    // 2. Injecting an Interface from the category module's ROOT package
    private final CategoryInternalAPI categoryInternalAPI; 

    public List<TransactionDTO> getUserTransactions(UUID userId) {
        // Use userId instead of user object
        return transactionRepository.findAllByUserIdOrderByTransactionDateDesc(userId)
                .stream()
                .map(this::convertToDto)
                .collect(Collectors.toList());
    }

    public List<TransactionDTO> getTransactionsByPeriod(UUID userId, LocalDate start, LocalDate end) {
        return transactionRepository.findAllByUserIdAndTransactionDateBetweenOrderByTransactionDateDesc(userId, start, end)
                .stream()
                .map(this::convertToDto)
                .collect(Collectors.toList());
    }

    public BigDecimal getTotalExpense(UUID userId) {
        BigDecimal total = transactionRepository.sumTotalByType(userId, TransactionType.DEBIT);
        return total != null ? total : BigDecimal.ZERO;
    }

    public BigDecimal getTotalIncome(UUID userId) {
        BigDecimal total = transactionRepository.sumTotalByType(userId, TransactionType.CREDIT);
        return total != null ? total : BigDecimal.ZERO;
    }

    public TransactionSummaryDTO getTransactionSummary(UUID userId) {
        BigDecimal income = getTotalIncome(userId);
        BigDecimal expense = getTotalExpense(userId);
        return TransactionSummaryDTO.builder()
                .totalIncome(income)
                .totalExpense(expense)
                .totalBalance(income.subtract(expense))
                .build();
    }

    public BigDecimal getExpenseByCategory(UUID userId, UUID categoryId) {
        BigDecimal total = transactionRepository.sumByCategoryAndType(userId, categoryId, TransactionType.DEBIT);
        return total != null ? total : BigDecimal.ZERO;
    }

    @Transactional
    public TransactionDTO createTransaction(TransactionDTO dto, UUID userId) {
        // 3. Validation happens via the exposed API, not the hidden repository
        if (!categoryInternalAPI.existsByIdAndUserId(dto.getCategoryId(), userId)) {
             throw new ResourceNotFoundException("Category not found or unauthorized.");
        }

        Transaction transaction = Transaction.builder()
                .title(dto.getTitle())
                .description(dto.getDescription())
                .amount(dto.getAmount())
                .type(dto.getType())
                .transactionDate(dto.getTransactionDate())
                .categoryId(dto.getCategoryId()) // Store ID
                .userId(userId) // Store ID
                .build();

        Transaction saved = transactionRepository.save(transaction);
        return convertToDto(saved);
    }

    @Transactional
    public TransactionDTO updateTransaction(UUID id, TransactionDTO dto, UUID userId) {
        Transaction transaction = transactionRepository.findById(id)
                .filter(t -> t.getUserId().equals(userId))
                .orElseThrow(() -> new ResourceNotFoundException("Transaction not found or unauthorized."));

        if (!categoryInternalAPI.existsByIdAndUserId(dto.getCategoryId(), userId)) {
            throw new ResourceNotFoundException("Category not found or unauthorized.");
        }

        transaction.setTitle(dto.getTitle());
        transaction.setDescription(dto.getDescription());
        transaction.setAmount(dto.getAmount());
        transaction.setType(dto.getType());
        transaction.setTransactionDate(dto.getTransactionDate());
        transaction.setCategoryId(dto.getCategoryId());

        return convertToDto(transactionRepository.save(transaction));
    }

    @Transactional
    public void deleteTransaction(UUID id, UUID userId) {
        Transaction transaction = transactionRepository.findById(id)
                .filter(t -> t.getUserId().equals(userId))
                .orElseThrow(() -> new ResourceNotFoundException("Transaction not found or unauthorized."));
        transactionRepository.delete(transaction);
    }

    private TransactionDTO convertToDto(Transaction transaction) {
        TransactionDTO dto = modelMapper.map(transaction, TransactionDTO.class);
        
        // 4. To get name/icon, we call the API instead of traversing the object graph
        var categoryMetadata = categoryInternalAPI.getCategoryMetadata(transaction.getCategoryId());
        dto.setCategoryName(categoryMetadata.name());
        dto.setCategoryIcon(categoryMetadata.icon());
        
        return dto;
    }
}